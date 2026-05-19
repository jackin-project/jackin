use std::io::Write as _;

use crate::docker::{CommandRunner, RunOptions};
use crate::docker_client::DockerApi;
use crate::instance::InstanceManifest;

pub use crate::docker_client::ContainerState;
#[cfg(test)]
use crate::instance::{InstanceIndex, InstanceStatus};
use crate::paths::JackinPaths;

use super::naming::dind_certs_volume;

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

pub async fn inspect_agent_sessions(
    docker: &impl DockerApi,
    container_name: &str,
    state: &ContainerState,
) -> AgentSessionInventory {
    if matches!(state, ContainerState::InspectUnavailable(_)) {
        return AgentSessionInventory::Unavailable(
            "container state unavailable; skipping session query".to_string(),
        );
    }
    if !matches!(state, ContainerState::Running) {
        return AgentSessionInventory::NotRunning;
    }

    // `tmux list-sessions` exits 1 when no sessions exist, which docker exec
    // surfaces as an error. Running via `sh -c '... || true'` maps both "zero
    // sessions" and "sessions found" to exit 0; only a real infrastructure
    // failure (container stopped mid-call, docker unavailable) reaches `Err`.
    match docker
        .exec_capture(
            container_name,
            &[
                "sh",
                "-c",
                "tmux list-sessions -F '#{session_name}' 2>/dev/null || true",
            ],
        )
        .await
    {
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
pub(super) async fn reconnect_or_create_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    set_role_terminal_title(paths, container_name);
    let sessions = inspect_agent_sessions(docker, container_name, &ContainerState::Running).await;
    let has_sessions = matches!(&sessions, AgentSessionInventory::Sessions(v) if !v.is_empty());

    if has_sessions {
        runner
            .run(
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
            .await
    } else {
        let agent_slug =
            crate::instance::InstanceManifest::read(&paths.data_dir.join(container_name))
                .ok()
                .and_then(|m| m.agent().ok())
                .map_or_else(|| "agent".to_string(), |a| a.slug().to_string());
        let agent_env = format!("{}={agent_slug}", crate::env_model::JACKIN_AGENT_ENV_NAME);
        let session_name = format!("jackin-{agent_slug}-{}", short_session_id());
        runner
            .run(
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
            .await
    }
}

/// Open a one-shot interactive zsh shell in a running container.
///
/// Intentionally ephemeral — no tmux session, no reconnect. Used by
/// `jackin hardline --shell` and the console Shell action.
pub async fn spawn_shell_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl crate::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    match docker.inspect_container_state(container_name).await {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {}
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
        ContainerState::Stopped { .. }
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => {
            anyhow::bail!(
                "container '{container_name}' is stopped; run `jackin hardline {container_name}` to restart it before opening a shell"
            );
        }
    }

    set_role_terminal_title(paths, container_name);
    super::caffeinate::reconcile(paths, docker, runner).await;
    let result = runner
        .run(
            "docker",
            &["exec", "-e", "TMUX=", "-it", container_name, "/bin/zsh"],
            None,
            &RunOptions::default(),
        )
        .await;
    eprintln!();
    result
}

#[allow(clippy::too_many_arguments)]
pub async fn spawn_agent_session(
    paths: &JackinPaths,
    container_name: &str,
    manifest: Option<&InstanceManifest>,
    agent: crate::agent::Agent,
    git_coauthor_trailer: bool,
    git_dco: bool,
    docker: &impl crate::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    match docker.inspect_container_state(container_name).await {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {}
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
        ContainerState::Stopped { .. }
        | ContainerState::Created
        | ContainerState::Removing
        | ContainerState::Dead => {
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
    let git_coauthor_trailer_env = git_coauthor_trailer.then(|| {
        format!(
            "{}=1",
            crate::env_model::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME
        )
    });
    let git_dco_env = git_dco.then(|| format!("{}=1", crate::env_model::JACKIN_GIT_DCO_ENV_NAME));
    let session_name = format!("jackin-{}-{}", agent.slug(), short_session_id());
    set_role_terminal_title(paths, container_name);
    super::caffeinate::reconcile(paths, docker, runner).await;
    let mut tmux_args = vec![
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
    ];
    if let Some(ref env) = git_coauthor_trailer_env {
        tmux_args.extend_from_slice(&["-e", env.as_str()]);
    }
    if let Some(ref env) = git_dco_env {
        tmux_args.extend_from_slice(&["-e", env.as_str()]);
    }
    tmux_args.extend_from_slice(&["-s", &session_name, "--", "/jackin/runtime/entrypoint.sh"]);
    let result = runner
        .run("docker", &tmux_args, None, &RunOptions::default())
        .await;
    eprintln!();
    result?;

    let outcome = crate::runtime::launch::inspect_attach_outcome(docker, container_name).await?;
    super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    Ok(())
}

pub async fn hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl crate::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    // Reconcile keep_awake right before each `reconnect_or_create_session` call.
    // `reconnect_or_create_session` blocks on the tmux exec until the session ends,
    // so the post-hardline reconcile in `app::Command::Hardline` would fire
    // too late. Firing here, while the container is observably running, ensures
    // caffeinate spawns for the duration of the re-attached session.
    let attach_outcome = match docker.inspect_container_state(container_name).await {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            super::caffeinate::reconcile(paths, docker, runner).await;
            reconnect_or_create_session(paths, container_name, docker, runner).await
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
        state @ (ContainerState::Created | ContainerState::Removing | ContainerState::Dead) => {
            anyhow::bail!(
                "container '{container_name}' is not running (state: {}); \
                 use `jackin load` to start a new session",
                state.short_label()
            )
        }
    };
    attach_outcome?;

    // Finalize per-mount isolation worktrees after re-attach. We do not honor
    // a `ReturnToAgent` decision here — `hardline` is itself a re-attach, and
    // the operator can simply re-invoke `jackin hardline` to come back.
    let outcome = crate::runtime::launch::inspect_attach_outcome(docker, container_name).await?;
    super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let mut prompt = crate::isolation::finalize::StdinPrompt;
    let _ = crate::isolation::finalize::finalize_foreground_session(
        container_name,
        &paths.data_dir.join(container_name),
        outcome,
        interactive,
        &mut prompt,
        docker,
        runner,
    )
    .await?;
    Ok(())
}

pub async fn inspect_hardline_instance(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl crate::docker_client::DockerApi,
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

    let (role_container_state, dind_state_raw, network_result) = tokio::join!(
        docker.inspect_container_state(container_name),
        docker.inspect_container_state(&dind_name),
        inspect_docker_network(docker, &network_name),
    );
    let sessions = inspect_agent_sessions(docker, container_name, &role_container_state).await;
    let role_state = role_container_state.inspect_label();
    let dind_state = dind_state_raw.inspect_label();
    let network_state = describe_network_state(network_result);
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

async fn inspect_docker_network(
    docker: &impl crate::docker_client::DockerApi,
    network: &str,
) -> DockerNetworkState {
    match docker.inspect_network(network).await {
        Ok(Some(_)) => DockerNetworkState::Present,
        Ok(None) => DockerNetworkState::NotFound,
        Err(e) => DockerNetworkState::InspectUnavailable(e.to_string()),
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

pub(super) async fn wait_for_dind(
    dind_name: &str,
    certs_volume: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    const MAX_ATTEMPTS: u32 = 30;
    const INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
    const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    const SPIN_MS: u64 = 80;
    let mut frame_idx: usize = 0;
    let message = "Waiting for Docker-in-Docker to be ready";
    let mut last_err = None;

    for _ in 0..MAX_ATTEMPTS {
        match docker.exec_capture(dind_name, &["docker", "info"]).await {
            Ok(_) => {
                eprint!("\r\x1b[2K");
                let _ = std::io::stderr().flush();
                last_err = None;
                break;
            }
            Err(e) => last_err = Some(e),
        }
        let spins = INTERVAL.as_millis() as u64 / SPIN_MS;
        for _ in 0..spins {
            let frame = FRAMES[frame_idx % FRAMES.len()];
            eprint!("\r   {frame}   {message}");
            let _ = std::io::stderr().flush();
            tokio::time::sleep(std::time::Duration::from_millis(SPIN_MS)).await;
            frame_idx += 1;
        }
    }
    eprint!("\r\x1b[2K");
    let _ = std::io::stderr().flush();

    if let Some(e) = last_err {
        return Err(anyhow::anyhow!(
            "timed out waiting for Docker-in-Docker sidecar {dind_name}: {e}"
        ));
    }

    match docker
        .exec_capture(dind_name, &["test", "-f", "/certs/client/ca.pem"])
        .await
    {
        Ok(_) => {}
        Err(e) if e.to_string().contains("exited with code") => {
            anyhow::bail!(
                "DinD TLS client certificates not found on volume {certs_volume} — \
                 the DinD sidecar may have started without generating certificates"
            );
        }
        Err(e) => return Err(e.context(format!("checking TLS cert presence in {dind_name}"))),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::test_support::FakeRunner;
    use super::*;
    use crate::docker_client::FakeDockerClient;
    use tempfile::TempDir;

    fn test_paths() -> (TempDir, JackinPaths) {
        let dir = TempDir::new().unwrap();
        let paths = JackinPaths::for_tests(dir.path());
        (dir, paths)
    }

    #[tokio::test]
    async fn hardline_attaches_when_container_is_running() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap();

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

    #[tokio::test]
    async fn hardline_new_session_execs_entrypoint_in_running_container() {
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
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
                ContainerState::Running,
                ContainerState::Running,
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        spawn_agent_session(
            &paths,
            container_name,
            Some(&manifest),
            crate::agent::Agent::Codex,
            false,
            false,
            &docker,
            &mut runner,
        )
        .await
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

    #[tokio::test]
    async fn hardline_new_session_forwards_coauthor_trailer_env_when_enabled() {
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
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
                ContainerState::Running,
                ContainerState::Running,
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        spawn_agent_session(
            &paths,
            container_name,
            Some(&manifest),
            crate::agent::Agent::Claude,
            true,
            false,
            &docker,
            &mut runner,
        )
        .await
        .unwrap();

        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("-e JACKIN_GIT_COAUTHOR_TRAILER=1")),
            "coauthor trailer env must be present when enabled; recorded: {:?}",
            runner.recorded
        );
    }

    #[tokio::test]
    async fn hardline_new_session_forwards_dco_env_when_enabled() {
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
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
                ContainerState::Running,
                ContainerState::Running,
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        spawn_agent_session(
            &paths,
            container_name,
            Some(&manifest),
            crate::agent::Agent::Claude,
            false,
            true,
            &docker,
            &mut runner,
        )
        .await
        .unwrap();

        assert!(
            runner
                .recorded
                .iter()
                .any(|call| call.contains("-e JACKIN_GIT_DCO=1")),
            "DCO env must be present when enabled; recorded: {:?}",
            runner.recorded
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("JACKIN_GIT_COAUTHOR_TRAILER")),
            "coauthor trailer env must be absent when disabled; recorded: {:?}",
            runner.recorded
        );
    }

    #[tokio::test]
    async fn hardline_new_session_requires_running_container() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Stopped {
                    exit_code: 137,
                    oom_killed: false,
                },
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = spawn_agent_session(
            &paths,
            "jk-agent-smith",
            None,
            crate::agent::Agent::Claude,
            false,
            false,
            &docker,
            &mut runner,
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("is stopped"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.starts_with("docker exec"))
        );
    }

    #[tokio::test]
    async fn spawn_shell_session_execs_zsh_in_running_container() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap();

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

    #[tokio::test]
    async fn spawn_shell_session_sets_tmux_env_to_empty() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap();

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

    #[tokio::test]
    async fn spawn_shell_session_errors_on_stopped_container() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Stopped {
                    exit_code: 137,
                    oom_killed: false,
                },
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("is stopped"));
        assert!(
            !runner.recorded.iter().any(|c| c.contains("docker exec")),
            "exec must not fire against a stopped container"
        );
    }

    #[tokio::test]
    async fn spawn_shell_session_errors_on_not_found() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient::default(); // empty inspect → NotFound
        let mut runner = FakeRunner::default();

        let err = spawn_shell_session(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("not found"));
        assert!(!runner.recorded.iter().any(|c| c.contains("docker exec")));
    }

    #[tokio::test]
    async fn hardline_errors_when_container_not_found() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient::default();
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("not found"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux new-session"))
        );
    }

    #[tokio::test]
    async fn hardline_errors_when_docker_inspect_is_unavailable() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            fail_with: vec![(
                "docker inspect jk-agent-smith".to_string(),
                "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
            )],
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("Docker is unavailable"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux new-session"))
        );
    }

    #[tokio::test]
    async fn hardline_marks_missing_manifest_restore_available() {
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
        let docker = FakeDockerClient::default(); // NotFound
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, container_name, &docker, &mut runner)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("state remains recoverable"));
        let manifest = InstanceManifest::read(&state_dir).unwrap();
        assert_eq!(manifest.status, InstanceStatus::RestoreAvailable);
        let index = InstanceIndex::read_or_rebuild(&paths.data_dir).unwrap();
        assert_eq!(index.instances[0].status, InstanceStatus::RestoreAvailable);
    }

    #[tokio::test]
    async fn inspect_hardline_instance_reports_state_without_attaching() {
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
        // inspect: role container running, dind stopped
        // exec_capture: tmux list-sessions returns two sessions
        // inspect_network: network present
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
                ContainerState::Stopped {
                    exit_code: 137,
                    oom_killed: false,
                },
            ])),
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                "jackin-claude-abc123\njackin-codex-abc".to_string(),
            ])),
            inspect_network_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                Some(crate::docker_client::NetworkRow {
                    name: format!("{container_name}-net"),
                    labels: HashMap::default(),
                }),
            ])),
            ..Default::default()
        };
        let report = inspect_hardline_instance(&paths, container_name, &docker)
            .await
            .unwrap();

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
    }

    #[tokio::test]
    async fn inspect_agent_sessions_lists_tmux_sessions() {
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                "jackin-claude-abc123\njackin-codex-abc".to_string(),
            ])),
            ..Default::default()
        };

        let sessions =
            inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

        let AgentSessionInventory::Sessions(sessions) = sessions else {
            panic!("expected sessions");
        };
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].command, "jackin-claude-abc123");
        assert_eq!(sessions[1].command, "jackin-codex-abc");
    }

    #[tokio::test]
    async fn inspect_agent_sessions_returns_empty_when_no_sessions_running() {
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                String::new(),
            ])),
            ..Default::default()
        };

        let sessions =
            inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

        assert_eq!(sessions, AgentSessionInventory::Sessions(vec![]));
    }

    #[tokio::test]
    async fn inspect_agent_sessions_skips_query_when_container_is_not_running() {
        let docker = FakeDockerClient::default();

        let sessions = inspect_agent_sessions(
            &docker,
            "jk-agent-smith",
            &ContainerState::Stopped {
                exit_code: 137,
                oom_killed: false,
            },
        )
        .await;

        assert_eq!(sessions, AgentSessionInventory::NotRunning);
        assert!(docker.recorded.borrow().is_empty());
    }

    #[tokio::test]
    async fn inspect_hardline_instance_still_reports_manifest_when_docker_unavailable() {
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
        let docker = FakeDockerClient {
            fail_with: vec![(
                "docker inspect jk-k7p9m2xq-workspace-agentsmith".to_string(),
                "Cannot connect to the Docker daemon at unix:///var/run/docker.sock".to_string(),
            )],
            ..Default::default()
        };
        let report = inspect_hardline_instance(&paths, container_name, &docker)
            .await
            .unwrap();

        assert!(report.contains("Workspace: workspace"), "{report}");
        assert!(report.contains("Role container: jk-k7p9m2xq-workspace-agentsmith (unavailable:"));
    }

    #[tokio::test]
    async fn hardline_errors_on_clean_exit() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Stopped {
                    exit_code: 0,
                    oom_killed: false,
                },
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("exited cleanly"));
        assert!(
            !runner
                .recorded
                .iter()
                .any(|c| c.contains("docker start") || c.contains("tmux new-session"))
        );
    }

    #[tokio::test]
    async fn hardline_refuses_crashed_container() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Stopped {
                    exit_code: 137,
                    oom_killed: false,
                },
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();

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

    #[tokio::test]
    async fn hardline_refuses_oom_killed_container() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Stopped {
                    exit_code: 0,
                    oom_killed: true,
                },
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("OOM") && err.to_string().contains("jackin load"),
            "expected OOM error directing to jackin load; got: {err}"
        );
    }

    #[tokio::test]
    async fn wait_for_dind_times_out_when_all_attempts_fail() {
        tokio::time::pause(); // make all sleeps instant
        let docker = FakeDockerClient {
            fail_with: vec![("docker exec".to_string(), "connection refused".to_string())],
            ..Default::default()
        };

        let err = wait_for_dind("jk-agent-smith-dind", "jk-agent-smith-dind-certs", &docker)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("timed out"), "got: {err}");
    }

    #[tokio::test]
    async fn wait_for_dind_fails_when_cert_absent() {
        // First exec (docker info) succeeds; second exec (test -f) exits with code 1.
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                // docker info: success
                String::new(),
            ])),
            fail_with: vec![(
                "test -f /certs/client/ca.pem".to_string(),
                "exec in jk-agent-smith-dind exited with code 1: ".to_string(),
            )],
            ..Default::default()
        };

        let err = wait_for_dind("jk-agent-smith-dind", "jk-agent-smith-dind-certs", &docker)
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("TLS client certificates not found"),
            "got: {err}"
        );
    }
}
