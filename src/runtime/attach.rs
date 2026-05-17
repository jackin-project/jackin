use crate::docker::{CommandRunner, RunOptions};
use crate::instance::InstanceManifest;
#[cfg(test)]
use crate::instance::{InstanceIndex, InstanceStatus};
use crate::paths::JackinPaths;
use crate::tui;

use super::naming::dind_certs_volume;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerState {
    NotFound,
    InspectUnavailable(String),
    Running,
    Stopped { exit_code: i32, oom_killed: bool },
}

impl ContainerState {
    /// Short label that elides the `InspectUnavailable` reason.
    #[must_use]
    pub fn short_label(&self) -> String {
        match self {
            Self::Running => "running".to_string(),
            Self::Stopped {
                exit_code,
                oom_killed: false,
            } => format!("stopped exit:{exit_code}"),
            Self::Stopped {
                oom_killed: true, ..
            } => "stopped oom_killed".to_string(),
            Self::NotFound => "missing".to_string(),
            Self::InspectUnavailable(_) => "unavailable".to_string(),
        }
    }

    /// Verbose label that surfaces the inspect-failure reason.
    #[must_use]
    pub fn inspect_label(&self) -> String {
        match self {
            Self::InspectUnavailable(reason) => format!("unavailable: {reason}"),
            _ => self.short_label(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionInventory {
    NotRunning,
    Unavailable(String),
    Sessions(Vec<AgentSession>),
}

/// Query a container's state with a single `docker inspect` call.
///
/// Uses Go-template formatting to extract three fields in one round trip:
/// `Running`, `ExitCode`, and `OOMKilled`.
pub fn inspect_container_state(runner: &mut impl CommandRunner, name: &str) -> ContainerState {
    let output = match runner.capture(
        "docker",
        &[
            "inspect",
            "--format",
            "{{.State.Running}} {{.State.ExitCode}} {{.State.OOMKilled}}",
            name,
        ],
        None,
    ) {
        Ok(output) => output,
        Err(error) => {
            let error = error.to_string();
            if crate::docker::is_missing_resource_error(&error) {
                return ContainerState::NotFound;
            }
            return ContainerState::InspectUnavailable(error);
        }
    };

    let mut parts = output.split_whitespace();
    let Some(running) = parts.next() else {
        return ContainerState::NotFound;
    };
    if running == "true" {
        return ContainerState::Running;
    }
    if running != "false" {
        return ContainerState::InspectUnavailable(format!(
            "unexpected docker inspect output for '{name}': {output:?}"
        ));
    }
    let Some(exit_code) = parts.next().and_then(|s| s.parse().ok()) else {
        return ContainerState::InspectUnavailable(format!(
            "unexpected docker inspect output for '{name}': {output:?}"
        ));
    };
    let Some(oom_killed) = parts.next().map(|part| part == "true") else {
        return ContainerState::InspectUnavailable(format!(
            "unexpected docker inspect output for '{name}': {output:?}"
        ));
    };
    ContainerState::Stopped {
        exit_code,
        oom_killed,
    }
}

pub fn inspect_agent_sessions(
    runner: &mut impl CommandRunner,
    container_name: &str,
    state: &ContainerState,
) -> AgentSessionInventory {
    if !matches!(state, ContainerState::Running) {
        return AgentSessionInventory::NotRunning;
    }

    // `tmux list-sessions` exits 1 when no sessions exist, which docker exec
    // surfaces as an error. Running via `sh -c '... || true'` maps both "zero
    // sessions" and "sessions found" to exit 0; only a real infrastructure
    // failure (container stopped mid-call, docker unavailable) reaches `Err`.
    match runner.capture(
        "docker",
        &[
            "exec",
            container_name,
            "sh",
            "-c",
            "tmux list-sessions -F '#{session_name}' 2>/dev/null || true",
        ],
        None,
    ) {
        Ok(output) => AgentSessionInventory::Sessions(parse_tmux_sessions(&output)),
        Err(error) => AgentSessionInventory::Unavailable(error.to_string()),
    }
}

fn parse_tmux_sessions(output: &str) -> Vec<AgentSession> {
    output
        .lines()
        .filter_map(|line| {
            let name = line.trim();
            if name.is_empty() {
                return None;
            }
            Some(AgentSession {
                command: name.to_string(),
            })
        })
        .collect()
}

/// Builder for `docker inspect`-failure operator messages. `clause`
/// is the verb + target phrase (e.g. ``"inspect container `foo`"``,
/// ``"claim container name `foo`"``); the tail is the shared
/// reason-suffix every call site needs.
pub fn docker_unavailable_msg(clause: &str, reason: &str) -> String {
    format!(
        "cannot {clause} because Docker is unavailable or returned an unexpected response: {reason}"
    )
}

fn inspect_unavailable_message(container_name: &str, reason: &str) -> String {
    docker_unavailable_msg(&format!("inspect container `{container_name}`"), reason)
}

fn set_role_terminal_title(paths: &JackinPaths, container_name: &str) {
    let title = InstanceManifest::read(&paths.data_dir.join(container_name))
        .map_or_else(|_| container_name.to_string(), |m| m.role_display_name);
    crate::tui::set_terminal_title(&title);
}

/// 6-hex-digit session ID from nanoseconds — ~16M distinct values per ms.
pub(super) fn short_session_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    format!("{:06x}", ts & 0x00ff_ffff)
}

/// Reconnect to an existing tmux session in a running container, or create a
/// new one when none are running.
///
/// If one or more sessions exist, attaches to the most recently used one via
/// `tmux attach-session` (no `-t`). If no sessions are running, reads the
/// agent from the instance manifest and starts a new `jackin-<agent>-<id>`
/// session via `entrypoint.sh`.
///
/// `TMUX=` prevents nested-session warnings when the operator's host terminal
/// is itself inside tmux.
pub(super) fn reconnect_or_create_session(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    set_role_terminal_title(paths, container_name);
    let sessions = inspect_agent_sessions(runner, container_name, &ContainerState::Running);
    let has_sessions = matches!(&sessions, AgentSessionInventory::Sessions(v) if !v.is_empty());

    if has_sessions {
        runner.run(
            "docker",
            &[
                "exec",
                "-e",
                "TMUX=",
                "-it",
                container_name,
                "tmux",
                "attach-session",
            ],
            None,
            &RunOptions::default(),
        )
    } else {
        let agent_slug =
            crate::instance::InstanceManifest::read(&paths.data_dir.join(container_name))
                .ok()
                .and_then(|m| m.agent().ok())
                .map_or_else(|| "agent".to_string(), |a| a.slug().to_string());
        let agent_env = format!("{}={agent_slug}", crate::env_model::JACKIN_AGENT_ENV_NAME);
        let session_name = format!("jackin-{agent_slug}-{}", short_session_id());
        runner.run(
            "docker",
            &[
                "exec",
                "-e",
                "TMUX=",
                "-it",
                container_name,
                "tmux",
                "new-session",
                "-e",
                &agent_env,
                "-s",
                &session_name,
                "--",
                "/jackin/runtime/entrypoint.sh",
            ],
            None,
            &RunOptions::default(),
        )
    }
}

/// Open a one-shot interactive zsh shell in a running container.
///
/// Intentionally ephemeral — no tmux session, no reconnect. Used by
/// `jackin hardline --shell` and the console Shell action.
pub fn spawn_shell_session(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    match inspect_container_state(runner, container_name) {
        ContainerState::Running => {}
        ContainerState::NotFound => {
            if let Some(message) = missing_restore_message(paths, container_name)? {
                anyhow::bail!("{message}");
            }
            anyhow::bail!(
                "container '{container_name}' not found; use `jackin load` to start a new session"
            );
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!("{}", inspect_unavailable_message(container_name, &reason));
        }
        ContainerState::Stopped { .. } => {
            anyhow::bail!(
                "container '{container_name}' is stopped; run `jackin hardline {container_name}` to restart it before opening a shell"
            );
        }
    }

    set_role_terminal_title(paths, container_name);
    super::caffeinate::reconcile(paths, runner);
    let result = runner.run(
        "docker",
        &["exec", "-e", "TMUX=", "-it", container_name, "/bin/zsh"],
        None,
        &RunOptions::default(),
    );
    eprintln!();
    result
}

pub fn spawn_agent_session(
    paths: &JackinPaths,
    container_name: &str,
    manifest: Option<&InstanceManifest>,
    agent: crate::agent::Agent,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    match inspect_container_state(runner, container_name) {
        ContainerState::Running => {}
        ContainerState::NotFound => {
            if let Some(message) = missing_restore_message(paths, container_name)? {
                anyhow::bail!("{message}");
            }
            anyhow::bail!(
                "container '{container_name}' not found; use `jackin load` to start a new session"
            );
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!("{}", inspect_unavailable_message(container_name, &reason));
        }
        ContainerState::Stopped { .. } => {
            anyhow::bail!(
                "container '{container_name}' is stopped; run `jackin hardline {container_name}` to restart or recover it before using `--new`"
            );
        }
    }

    let workdir = manifest.map_or("/workspace", |manifest| manifest.workdir.as_str());
    let agent_env = format!(
        "{}={}",
        crate::env_model::JACKIN_AGENT_ENV_NAME,
        agent.slug()
    );
    let session_name = format!("jackin-{}-{}", agent.slug(), short_session_id());
    set_role_terminal_title(paths, container_name);
    super::caffeinate::reconcile(paths, runner);
    let result = runner.run(
        "docker",
        &[
            "exec",
            "-e",
            "TMUX=",
            "--workdir",
            workdir,
            "-it",
            container_name,
            "tmux",
            "new-session",
            "-e",
            &agent_env,
            "-s",
            &session_name,
            "--",
            "/jackin/runtime/entrypoint.sh",
        ],
        None,
        &RunOptions::default(),
    );
    eprintln!();
    result?;

    let outcome = crate::runtime::launch::inspect_attach_outcome(runner, container_name)?;
    super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    Ok(())
}

pub fn hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    // Reconcile keep_awake right before each `reconnect_or_create_session` call.
    // `reconnect_or_create_session` blocks on the tmux exec until the session ends,
    // so the post-hardline reconcile in `app::Command::Hardline` would fire
    // too late. Firing here, while the container is observably running, ensures
    // caffeinate spawns for the duration of the re-attached session.
    let attach_outcome = match inspect_container_state(runner, container_name) {
        ContainerState::Running => {
            super::caffeinate::reconcile(paths, runner);
            reconnect_or_create_session(paths, container_name, runner)
        }
        ContainerState::NotFound => {
            if let Some(message) = missing_restore_message(paths, container_name)? {
                anyhow::bail!("{message}");
            }
            anyhow::bail!(
                "container '{container_name}' not found; use `jackin load` to start a new session"
            )
        }
        ContainerState::InspectUnavailable(reason) => {
            anyhow::bail!("{}", inspect_unavailable_message(container_name, &reason))
        }
        ContainerState::Stopped {
            exit_code: 0,
            oom_killed: false,
        } => {
            anyhow::bail!(
                "container '{container_name}' exited cleanly; \
                 use `jackin load` to start a new session"
            )
        }
        ContainerState::Stopped {
            exit_code,
            oom_killed,
        } => {
            let reason = if oom_killed {
                "OOM killed".to_string()
            } else {
                format!("exit {exit_code}")
            };
            anyhow::bail!(
                "container '{container_name}' stopped ({reason}); \
                 use `jackin load` to start a new session or recover saved state"
            )
        }
    };
    attach_outcome?;

    // Finalize per-mount isolation worktrees after re-attach. We do not honor
    // a `ReturnToAgent` decision here — `hardline` is itself a re-attach, and
    // the operator can simply re-invoke `jackin hardline` to come back.
    let outcome = crate::runtime::launch::inspect_attach_outcome(runner, container_name)?;
    super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let mut prompt = crate::isolation::finalize::StdinPrompt;
    let _ = crate::isolation::finalize::finalize_foreground_session(
        container_name,
        &paths.data_dir.join(container_name),
        outcome,
        interactive,
        &mut prompt,
        runner,
    )?;
    Ok(())
}

pub fn inspect_hardline_instance(
    paths: &JackinPaths,
    container_name: &str,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<String> {
    let state_dir = paths.data_dir.join(container_name);
    // `--inspect` is the operator's recovery tool. Distinguish "no
    // manifest yet" (pre-restore) from "manifest unreadable" (torn
    // JSON) so the render below does not lie about the latter.
    let manifest_result: Result<Option<InstanceManifest>, String> =
        InstanceManifest::read_optional(&state_dir).map_err(|e| e.to_string());
    let manifest = manifest_result.as_ref().ok().and_then(Option::as_ref);
    let dind_name = manifest.map_or_else(
        || format!("{container_name}-dind"),
        |manifest| manifest.docker.dind_container.clone(),
    );
    let network_name = manifest.as_ref().map_or_else(
        || format!("{container_name}-net"),
        |manifest| manifest.docker.network.clone(),
    );
    let certs_volume = manifest.as_ref().map_or_else(
        || dind_certs_volume(container_name),
        |manifest| manifest.docker.certs_volume.clone(),
    );

    let role_container_state = inspect_container_state(runner, container_name);
    let sessions = inspect_agent_sessions(runner, container_name, &role_container_state);
    let role_state = role_container_state.inspect_label();
    let dind_state = inspect_container_state(runner, &dind_name).inspect_label();
    let network_state = describe_network_state(inspect_docker_network(runner, &network_name));
    let mounts = describe_mount_state(&state_dir);

    let mut lines = vec![
        format!("Instance: {container_name}"),
        format!("State directory: {}", state_dir.display()),
    ];
    match &manifest_result {
        Ok(Some(manifest)) => {
            lines.extend([
                format!("Instance ID: {}", manifest.instance_id),
                format!("Workspace: {}", manifest.workspace_label),
                format!("Role: {}", manifest.role_key),
                format!("Agent: {}", manifest.agent_runtime),
                format!("Status: {}", manifest.status.label()),
                format!("Updated: {}", manifest.updated_at),
            ]);
            if let Some(outcome) = &manifest.last_attach_outcome {
                lines.push(format!("Last attach outcome: {outcome}"));
            }
            if let Some(source_ref) = &manifest.role_source_ref {
                lines.push(format!(
                    "Role source: {} ({source_ref})",
                    manifest.role_source_git
                ));
            } else if !manifest.role_source_git.is_empty() {
                lines.push(format!("Role source: {}", manifest.role_source_git));
            }
        }
        Ok(None) => lines.push("Manifest: missing".to_string()),
        Err(error) => lines.push(format!("Manifest: unreadable ({error})")),
    }

    lines.extend([
        format!("Role container: {container_name} ({role_state})"),
        format!("Agent sessions: {}", describe_agent_sessions(&sessions)),
        format!("DinD container: {dind_name} ({dind_state})"),
        format!("Docker network: {network_name} ({network_state})"),
        format!("DinD cert volume: {certs_volume}"),
        format!("Mounts: {mounts}"),
    ]);
    Ok(lines.join("\n"))
}

pub fn describe_agent_session_count(sessions: &AgentSessionInventory) -> String {
    match sessions {
        AgentSessionInventory::NotRunning => "sessions:not_running".to_string(),
        AgentSessionInventory::Unavailable(_) => "sessions:unavailable".to_string(),
        AgentSessionInventory::Sessions(sessions) => format!("sessions:{}", sessions.len()),
    }
}

fn describe_agent_sessions(sessions: &AgentSessionInventory) -> String {
    match sessions {
        AgentSessionInventory::NotRunning => "not running".to_string(),
        AgentSessionInventory::Unavailable(reason) => format!("unavailable: {reason}"),
        AgentSessionInventory::Sessions(sessions) if sessions.is_empty() => {
            "none detected".to_string()
        }
        AgentSessionInventory::Sessions(sessions) => sessions
            .iter()
            .map(|session| session.command.as_str())
            .collect::<Vec<_>>()
            .join("; "),
    }
}

fn describe_network_state(state: DockerNetworkState) -> String {
    match state {
        DockerNetworkState::Present => "present".to_string(),
        DockerNetworkState::NotFound => "missing".to_string(),
        DockerNetworkState::InspectUnavailable(reason) => format!("unavailable: {reason}"),
    }
}

fn describe_mount_state(state_dir: &std::path::Path) -> String {
    crate::isolation::state::MountSummary::for_state_dir(state_dir).map_or_else(
        |_| "unknown".to_string(),
        crate::isolation::state::MountSummary::inspect_label,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerNetworkState {
    Present,
    NotFound,
    InspectUnavailable(String),
}

fn inspect_docker_network(runner: &mut impl CommandRunner, network: &str) -> DockerNetworkState {
    match runner.capture("docker", &["network", "inspect", network], None) {
        Ok(_) => DockerNetworkState::Present,
        Err(error) => {
            let error = error.to_string();
            if crate::docker::is_missing_resource_error(&error) {
                DockerNetworkState::NotFound
            } else {
                DockerNetworkState::InspectUnavailable(error)
            }
        }
    }
}

fn missing_restore_message(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<Option<String>> {
    let state_dir = paths.data_dir.join(container_name);
    let Some(mut manifest) = InstanceManifest::read_optional(&state_dir)? else {
        return Ok(None);
    };
    if !manifest.is_restore_candidate() {
        return Ok(None);
    }

    manifest.mark_restore_available(paths)?;
    Ok(Some(format!(
        "container '{container_name}' is missing, but jackin-managed local state remains recoverable at {}. \
         Run `jackin load` from the matching workspace to rebuild it, or `jackin eject {container_name} --purge` \
         to discard it. Anything written only to the deleted container's writable layer is gone and will not be restored, including ad-hoc package installs, global files outside mounted paths, and DinD images.",
        state_dir.display()
    )))
}

pub(super) fn wait_for_dind(
    dind_name: &str,
    certs_volume: &str,
    runner: &mut impl CommandRunner,
    _debug: bool,
) -> anyhow::Result<()> {
    // Wait for the DinD daemon to become ready (TLS handshake included).
    tui::spin_wait(
        "Waiting for Docker-in-Docker to be ready",
        30,
        std::time::Duration::from_secs(1),
        || {
            runner
                .capture("docker", &["exec", dind_name, "docker", "info"], None)
                .map(|_| ())
        },
    )
    .map_err(|_| anyhow::anyhow!("timed out waiting for Docker-in-Docker sidecar {dind_name}"))?;

    // Verify TLS client certificates were generated on the shared volume.
    // The DinD entrypoint writes certs before starting dockerd, so this
    // should succeed immediately after `docker info` passes.
    runner
        .capture(
            "docker",
            &["exec", dind_name, "test", "-f", "/certs/client/ca.pem"],
            None,
        )
        .map_err(|_| {
            anyhow::anyhow!(
                "DinD TLS client certificates not found on volume {certs_volume} — \
                 the DinD sidecar may have started without generating certificates"
            )
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::FakeRunner;
    use super::*;
    use tempfile::TempDir;

    fn test_paths() -> (TempDir, JackinPaths) {
        let dir = TempDir::new().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        (dir, paths)
    }

    #[test]
    fn hardline_attaches_when_container_is_running() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);

        hardline_agent(&paths, "jk-agent-smith", &mut runner).unwrap();

        assert!(
            runner.recorded.iter().any(|c| {
                c.contains("docker exec")
                    && c.contains("TMUX=")
                    && c.contains("jk-agent-smith")
                    && c.contains("tmux new-session")
                    && c.contains("jackin-")
            }),
            "expected tmux new-session exec in recorded commands; got: {:?}",
            runner.recorded
        );
    }

    #[test]
    fn hardline_new_session_execs_entrypoint_in_running_container() {
        let (_tmp, paths) = test_paths();
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace/project",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk-agent-smith",
            docker: crate::instance::DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        let mut runner = FakeRunner::with_capture_queue([
            "true 0 false".to_string(),
            "true 0 false".to_string(),
            "true 0 false".to_string(),
        ]);

        spawn_agent_session(
            &paths,
            container_name,
            Some(&manifest),
            crate::agent::Agent::Codex,
            &mut runner,
        )
        .unwrap();

        assert!(
            runner.recorded.iter().any(|call| {
                call.contains("docker exec")
                    && call.contains("TMUX=")
                    && call.contains("JACKIN_AGENT=codex")
                    && call.contains("--workdir /workspace/project")
                    && call.contains("jk-k7p9m2xq-workspace-agentsmith")
                    && call.contains("tmux new-session")
                    && call.contains("jackin-codex-")
                    && call.contains("/jackin/runtime/entrypoint.sh")
            }),
            "expected tmux new-session for codex; got: {:?}",
            runner.recorded
        );
    }

    #[test]
    fn hardline_new_session_requires_running_container() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["false 137 false".to_string()]);

        let err = spawn_agent_session(
            &paths,
            "jk-agent-smith",
            None,
            crate::agent::Agent::Claude,
            &mut runner,
        )
        .unwrap_err();

        assert!(err.to_string().contains("is stopped"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.starts_with("docker exec"))
        );
    }

    #[test]
    fn spawn_shell_session_execs_zsh_in_running_container() {
        let (_tmp, paths) = test_paths();
        // inspect returns Running; capture queue for caffeinate inspect.
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);

        spawn_shell_session(&paths, "jk-agent-smith", &mut runner).unwrap();

        assert!(
            runner.recorded.iter().any(|c| {
                c.contains("docker exec")
                    && c.contains("TMUX=")
                    && c.contains("jk-agent-smith")
                    && c.contains("/bin/zsh")
            }),
            "expected docker exec with /bin/zsh; got: {:?}",
            runner.recorded
        );
    }

    #[test]
    fn spawn_shell_session_sets_tmux_env_to_empty() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["true 0 false".to_string()]);

        spawn_shell_session(&paths, "jk-agent-smith", &mut runner).unwrap();

        let exec_call = runner
            .recorded
            .iter()
            .find(|c| c.contains("docker exec") && c.contains("/bin/zsh"))
            .expect("expected exec call");
        assert!(
            exec_call.contains("TMUX="),
            "TMUX= must clear nested-session env"
        );
    }

    #[test]
    fn spawn_shell_session_errors_on_stopped_container() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["false 137 false".to_string()]);

        let err = spawn_shell_session(&paths, "jk-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("is stopped"));
        assert!(
            !runner.recorded.iter().any(|c| c.contains("docker exec")),
            "exec must not fire against a stopped container"
        );
    }

    #[test]
    fn spawn_shell_session_errors_on_not_found() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::default(); // empty inspect → NotFound

        let err = spawn_shell_session(&paths, "jk-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("not found"));
        assert!(!runner.recorded.iter().any(|c| c.contains("docker exec")));
    }

    #[test]
    fn hardline_errors_when_container_not_found() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, "jk-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("not found"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux new-session"))
        );
    }

    #[test]
    fn inspect_container_state_distinguishes_missing_container_from_docker_failure() {
        let mut missing = FakeRunner::default();
        missing.fail_with.push((
            "docker inspect".to_string(),
            "Error: No such object: jk-agent-smith".to_string(),
        ));
        assert_eq!(
            inspect_container_state(&mut missing, "jk-agent-smith"),
            ContainerState::NotFound
        );

        let mut unavailable = FakeRunner::default();
        unavailable.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));
        assert!(matches!(
            inspect_container_state(&mut unavailable, "jk-agent-smith"),
            ContainerState::InspectUnavailable(reason)
                if reason.contains("Cannot connect to the Docker daemon")
        ));
    }

    #[test]
    fn hardline_errors_when_docker_inspect_is_unavailable() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::default();
        runner.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));

        let err = hardline_agent(&paths, "jk-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("Docker is unavailable"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux new-session"))
        );
    }

    #[test]
    fn hardline_marks_missing_manifest_restore_available() {
        let (_tmp, paths) = test_paths();
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk-agent-smith",
            docker: crate::instance::DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        manifest.mark_status(InstanceStatus::Crashed);
        let state_dir = paths.data_dir.join(container_name);
        manifest.write(&state_dir).unwrap();
        InstanceIndex::update_manifest(&paths.data_dir, &manifest).unwrap();
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, container_name, &mut runner).unwrap_err();

        assert!(err.to_string().contains("state remains recoverable"));
        let manifest = InstanceManifest::read(&state_dir).unwrap();
        assert_eq!(manifest.status, InstanceStatus::RestoreAvailable);
        let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert_eq!(index.instances[0].status, InstanceStatus::RestoreAvailable);
    }

    #[test]
    fn inspect_hardline_instance_reports_state_without_attaching() {
        let (_tmp, paths) = test_paths();
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let mut manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Codex,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: Some("feature/role"),
            image_tag: "jk-agent-smith",
            docker: crate::instance::DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        manifest.mark_status(InstanceStatus::PreservedDirty);
        manifest.last_attach_outcome = Some("exit:137".to_string());
        manifest
            .write(&paths.data_dir.join(container_name))
            .unwrap();
        let mut runner = FakeRunner::with_capture_queue([
            "true 0 false".to_string(),
            "jackin-claude-abc123\njackin-codex-abc".to_string(),
            "false 137 false".to_string(),
            "[]".to_string(),
        ]);

        let report = inspect_hardline_instance(&paths, container_name, &mut runner).unwrap();

        assert!(report.contains("Instance ID: k7p9m2xq"), "{report}");
        assert!(report.contains("Workspace: workspace"), "{report}");
        assert!(report.contains("Role: agent-smith"), "{report}");
        assert!(report.contains("Agent: codex"), "{report}");
        assert!(report.contains("Status: preserved_dirty"), "{report}");
        assert!(report.contains("Last attach outcome: exit:137"), "{report}");
        assert!(
            report.contains("Agent sessions: jackin-claude-abc123; jackin-codex-abc"),
            "{report}"
        );
        assert!(report.contains("Role container: jk-k7p9m2xq-workspace-agentsmith (running)"));
        assert!(
            report.contains(
                "DinD container: jk-k7p9m2xq-workspace-agentsmith-dind (stopped exit:137)"
            )
        );
        assert!(report.contains("Docker network: jk-k7p9m2xq-workspace-agentsmith-net (present)"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux new-session -A"))
        );
    }

    #[test]
    fn inspect_agent_sessions_lists_tmux_sessions() {
        let mut runner =
            FakeRunner::with_capture_queue(["jackin-claude-abc123\njackin-codex-abc".to_string()]);

        let sessions =
            inspect_agent_sessions(&mut runner, "jk-agent-smith", &ContainerState::Running);

        let AgentSessionInventory::Sessions(sessions) = sessions else {
            panic!("expected sessions");
        };
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].command, "jackin-claude-abc123");
        assert_eq!(sessions[1].command, "jackin-codex-abc");
    }

    #[test]
    fn inspect_agent_sessions_returns_empty_when_no_sessions_running() {
        // tmux list-sessions exits 1 when no sessions exist; the sh wrapper
        // converts that to exit 0 with empty output → empty sessions list.
        let mut runner = FakeRunner::with_capture_queue([String::new()]);

        let sessions =
            inspect_agent_sessions(&mut runner, "jk-agent-smith", &ContainerState::Running);

        assert_eq!(sessions, AgentSessionInventory::Sessions(vec![]));
    }

    #[test]
    fn inspect_agent_sessions_skips_query_when_container_is_not_running() {
        let mut runner = FakeRunner::default();

        let sessions = inspect_agent_sessions(
            &mut runner,
            "jk-agent-smith",
            &ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
        );

        assert_eq!(sessions, AgentSessionInventory::NotRunning);
        assert!(runner.recorded.is_empty());
    }

    #[test]
    fn inspect_hardline_instance_still_reports_manifest_when_docker_unavailable() {
        let (_tmp, paths) = test_paths();
        let container_name = "jk-k7p9m2xq-workspace-agentsmith";
        let manifest = InstanceManifest::new(crate::instance::NewInstanceManifest {
            container_base: container_name,
            workspace_name: Some("workspace"),
            workspace_label: "workspace",
            workdir: "/workspace",
            host_workdir_fingerprint: "sha256:test",
            role_key: "agent-smith",
            role_display_name: "Agent Smith",
            agent_runtime: crate::agent::Agent::Claude,
            role_source_git: "https://example.invalid/agent-smith.git",
            role_source_ref: None,
            image_tag: "jk-agent-smith",
            docker: crate::instance::DockerResources {
                role_container: container_name.to_string(),
                dind_container: format!("{container_name}-dind"),
                network: format!("{container_name}-net"),
                certs_volume: format!("{container_name}-dind-certs"),
            },
        });
        manifest
            .write(&paths.data_dir.join(container_name))
            .unwrap();
        let mut runner = FakeRunner::default();
        runner.fail_with.push((
            "docker inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));
        runner.fail_with.push((
            "docker network inspect".to_string(),
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
        ));

        let report = inspect_hardline_instance(&paths, container_name, &mut runner).unwrap();

        assert!(report.contains("Workspace: workspace"), "{report}");
        assert!(report.contains("Role container: jk-k7p9m2xq-workspace-agentsmith (unavailable:"));
        assert!(
            report.contains("Docker network: jk-k7p9m2xq-workspace-agentsmith-net (unavailable:")
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("docker attach"))
        );
    }

    #[test]
    fn hardline_errors_on_clean_exit() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["false 0 false".to_string()]);

        let err = hardline_agent(&paths, "jk-agent-smith", &mut runner).unwrap_err();

        assert!(err.to_string().contains("exited cleanly"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux new-session"))
        );
    }

    #[test]
    fn hardline_refuses_crashed_container() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["false 137 false".to_string()]);

        let err = hardline_agent(&paths, "jk-agent-smith", &mut runner).unwrap_err();

        assert!(
            err.to_string().contains("stopped") && err.to_string().contains("jackin load"),
            "expected error directing to jackin load; got: {err}"
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux")),
            "hardline must not restart or attach stopped containers"
        );
    }

    #[test]
    fn hardline_refuses_oom_killed_container() {
        let (_tmp, paths) = test_paths();
        let mut runner = FakeRunner::with_capture_queue(["false 0 true".to_string()]);

        let err = hardline_agent(&paths, "jk-agent-smith", &mut runner).unwrap_err();

        assert!(
            err.to_string().contains("OOM") && err.to_string().contains("jackin load"),
            "expected OOM error directing to jackin load; got: {err}"
        );
    }
}
