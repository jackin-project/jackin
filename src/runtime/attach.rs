use crate::docker::{CommandRunner, RunOptions};
use crate::docker_client::DockerApi;
use crate::instance::InstanceManifest;
use anyhow::Context as _;

/// Shell command for querying the in-container daemon's session
/// inventory.
///
/// Gated on the daemon's socket file (`/jackin/run/jackin.sock`) so
/// the early-bring-up window — between container start and
/// `setup-once` finishing + the daemon binding its socket — does not
/// emit a wave of operator-visible stderr from a binary that exists
/// but cannot serve yet. `test -S` exits silently with status 1 if
/// the socket is absent, which `exec_capture` surfaces as `Err` and
/// callers route through `AgentSessionInventory::Unavailable`. Once
/// the socket is bound, every real failure mode of the status call
/// (daemon crashed mid-request, oversize reply, garbled JSON)
/// propagates loudly because `||` short-circuits at the first failure
/// only — there is no `|| true` suppression of the second command's
/// errors.
pub const JACKIN_STATUS_CMD: &str =
    "test -S /jackin/run/jackin.sock && /jackin/runtime/jackin-capsule status";

pub(super) async fn wait_for_capsule_daemon(
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    const MAX_ATTEMPTS: u32 = 60;
    const INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    crate::tui::prompt::spin_wait(
        "Waiting for jackin-capsule daemon",
        MAX_ATTEMPTS,
        INTERVAL,
        || async {
            docker
                .exec_capture(container_name, &["sh", "-c", JACKIN_STATUS_CMD])
                .await
                .map(|_| ())
        },
    )
    .await
    .with_context(|| format!("waiting for jackin-capsule daemon in {container_name}"))
}

pub use crate::docker_client::ContainerState;
#[cfg(test)]
use crate::instance::{InstanceIndex, InstanceStatus};
use crate::paths::JackinPaths;

use super::naming::dind_certs_volume;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub name: String,
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

    match docker
        .exec_capture(container_name, &["sh", "-c", JACKIN_STATUS_CMD])
        .await
    {
        Ok(output) => match parse_jackin_sessions(&output) {
            Ok(sessions) => AgentSessionInventory::Sessions(sessions),
            Err(reason) => AgentSessionInventory::Unavailable(reason),
        },
        Err(error) => AgentSessionInventory::Unavailable(error.to_string()),
    }
}

/// Parse the `Sessions: <N>` header from `jackin-capsule status`
/// output. Returns `None` if no parsable header line is present —
/// daemon unreachable, torn write, or post-format-drift. Shared
/// between `inspect_agent_sessions` here and
/// `isolation::finalize::has_jackin_sessions` so a future change to
/// the header shape touches one place, not two.
pub fn parse_session_count(output: &str) -> Option<usize> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Sessions:")
            .and_then(|value| value.trim().parse().ok())
    })
}

/// Parse session list from `jackin-capsule status` output.
///
/// The output starts with `Sessions: <N>` followed by N lines shaped
/// `  [<id>] <label> (<agent>) state=<state> active=<bool>`. The
/// header is required: without it, the function returns `Err` so
/// callers can route to `Unavailable` instead of silently treating
/// "no `[` lines" as "zero sessions". A cosmetic change to the
/// capsule's status print therefore surfaces immediately as an
/// operator-visible "sessions unavailable" rather than a wrong
/// auto-cleanup.
///
/// `take(expected)` consumes only the first N `[`-prefixed lines
/// after the header so a future trailing footer (totals row, debug
/// summary) or a label whose `Display` impl emits a non-`[` second
/// line does not flip the parse to `Unavailable`. Pre-header
/// `[`-prefixed lines are dropped by the `skip_while` synchronisation
/// on the header — that matches the capsule's print order, where the
/// header is always the first non-blank line.
fn parse_jackin_sessions(output: &str) -> Result<Vec<AgentSession>, String> {
    let expected = parse_session_count(output).ok_or_else(|| {
        "jackin-capsule status emitted no parsable `Sessions: N` header — daemon may be unreachable"
            .to_string()
    })?;

    let sessions: Vec<AgentSession> = output
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("Sessions:"))
        .skip(1)
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || !trimmed.starts_with('[') {
                return None;
            }
            // Strip from ` state=` onward, then strip the last
            // ` (<agent>)` block — what remains is the label. `rfind`
            // tolerates labels that themselves contain `(`.
            let after_id = trimmed.split(']').nth(1)?.trim_start();
            let head = after_id
                .rfind(" state=")
                .map_or(after_id, |idx| &after_id[..idx]);
            let name = head.rfind(" (").map_or(head, |idx| &head[..idx]);
            Some(AgentSession {
                name: name.to_string(),
            })
        })
        .take(expected)
        .collect();

    if sessions.len() < expected {
        return Err(format!(
            "jackin-capsule status header claims {expected} sessions but only {} `[`-prefixed lines parsed",
            sessions.len()
        ));
    }
    Ok(sessions)
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
    let title = match InstanceManifest::read(&paths.data_dir.join(container_name)) {
        Ok(m) => m.role_display_name,
        Err(e) => {
            crate::debug_log!(
                "attach",
                "set_role_terminal_title: manifest read failed for {container_name}: {e:#}; \
                 using container name as title",
            );
            container_name.to_string()
        }
    };
    crate::tui::set_terminal_title(&title);
}

/// Re-attach the operator's terminal to a running container's
/// daemon. When `focus_session` is `Some(id)`, the resulting
/// `docker exec` adds `--focus <id>` so the daemon honors the
/// host-supplied pane focus on its first Hello frame; `None` falls
/// through to "attach at whatever the daemon thinks is focused"
/// (the default reattach contract).
/// `docker exec` env flag that tells the in-container capsule client not to
/// toggle its own alternate screen, set only while the host orchestrator owns
/// one continuous alternate screen for the whole launch flow. Returns `None`
/// for standalone capsule invocations (e.g. `jackin hardline`), where the
/// client manages its own screen.
fn host_alt_screen_exec_flag() -> Option<&'static str> {
    crate::tui::host_screen_owned().then_some("-e=JACKIN_HOST_ALT_SCREEN=1")
}

pub(super) async fn reconnect_or_create_session_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    set_role_terminal_title(paths, container_name);
    wait_for_capsule_daemon(container_name, docker).await?;
    let focus_arg = focus_session.map(|id| id.to_string());
    let mut args: Vec<&str> = vec![
        "exec",
        "-it",
        container_name,
        "/jackin/runtime/jackin-capsule",
    ];
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    if let Some(ref id) = focus_arg {
        args.push("--focus");
        args.push(id);
    }
    runner
        .run("docker", &args, None, &RunOptions::default())
        .await
}

pub(super) async fn start_or_reconnect_capsule_client(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    match docker.inspect_container_state(container_name).await {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {}
        ContainerState::Stopped { .. } | ContainerState::Created => {
            docker
                .start_container(container_name)
                .await
                .with_context(|| format!("starting role container {container_name}"))?;
        }
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
        state @ (ContainerState::Removing | ContainerState::Dead) => {
            anyhow::bail!(
                "container '{container_name}' is not startable (state: {}); \
                 use `jackin load` to start a new session",
                state.short_label()
            );
        }
    }
    super::caffeinate::reconcile(paths, docker, runner).await;
    reconnect_or_create_session_with_focus(paths, container_name, None, docker, runner).await
}

/// Verify the container is reachable (running/paused/restarting).
/// Returns `Ok(())` when reachable, `Err` otherwise.
/// `stopped_hint` is the trailing clause of the "is stopped" error, e.g. "restart it before opening a shell".
async fn require_container_reachable(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl crate::docker_client::DockerApi,
    stopped_hint: &str,
) -> anyhow::Result<()> {
    match docker.inspect_container_state(container_name).await {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => Ok(()),
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
                "container '{container_name}' is stopped; run `jackin hardline {container_name}` to {stopped_hint}"
            );
        }
    }
}

/// Open a one-shot interactive zsh shell in a running container.
///
/// Ephemeral one-shot — no persistent session, no reconnect on detach. Used
/// by `jackin hardline --shell` and the console Shell action.
pub async fn spawn_shell_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl crate::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    require_container_reachable(
        paths,
        container_name,
        docker,
        "restart it before opening a shell",
    )
    .await?;

    set_role_terminal_title(paths, container_name);
    super::caffeinate::reconcile(paths, docker, runner).await;
    let mut args: Vec<&str> = vec![
        "exec",
        "-it",
        container_name,
        "/jackin/runtime/jackin-capsule",
        "new",
    ];
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    let result = runner
        .run("docker", &args, None, &RunOptions::default())
        .await;
    eprintln!();
    result?;
    finalize_reconnected_foreground_session(paths, container_name, docker, runner).await
}

#[expect(clippy::too_many_arguments)]
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
    require_container_reachable(
        paths,
        container_name,
        docker,
        "restart or recover it before using `--new`",
    )
    .await?;

    let workdir = manifest.map_or("/workspace", |manifest| manifest.workdir.as_str());

    // Agent selection travels as `jackin-capsule new <agent>` argv; these
    // env values are session policy toggles consumed by the spawned entrypoint.
    let coauthor_env = git_coauthor_trailer.then(|| {
        format!(
            "{}=1",
            crate::env_model::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME
        )
    });
    let dco_env = git_dco.then(|| format!("{}=1", crate::env_model::JACKIN_GIT_DCO_ENV_NAME));

    set_role_terminal_title(paths, container_name);
    super::caffeinate::reconcile(paths, docker, runner).await;

    let mut exec_args = vec![
        "exec",
        "--workdir",
        workdir,
        "-it",
        container_name,
        "/jackin/runtime/jackin-capsule",
        "new",
        agent.slug(),
    ];
    let coauthor_env_flag;
    let dco_env_flag;
    if let Some(ref env) = coauthor_env {
        coauthor_env_flag = format!("-e={env}");
        exec_args.insert(1, coauthor_env_flag.as_str());
    }
    if let Some(ref env) = dco_env {
        dco_env_flag = format!("-e={env}");
        exec_args.insert(1, dco_env_flag.as_str());
    }
    if let Some(flag) = host_alt_screen_exec_flag() {
        exec_args.insert(1, flag);
    }
    let result = runner
        .run("docker", &exec_args, None, &RunOptions::default())
        .await;
    eprintln!();
    result?;
    finalize_reconnected_foreground_session(paths, container_name, docker, runner).await
}

pub async fn hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl crate::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    hardline_agent_with_focus(paths, container_name, None, docker, runner).await
}

/// Same as `hardline_agent` but threads a host-supplied pane focus id.
///
/// The console preview navigation calls this with the
/// operator-selected pane so the reconnect lands inside that pane.
pub async fn hardline_agent_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl crate::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    // Reconcile keep_awake right before each `reconnect_or_create_session_with_focus`
    // call. The attach blocks on the jackin-capsule exec until the session ends,
    // so the post-hardline reconcile in `app::Command::Hardline` would fire
    // too late. Firing here, while the container is observably running, ensures
    // caffeinate spawns for the duration of the re-attached session.
    let attach_outcome = match docker.inspect_container_state(container_name).await {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            super::caffeinate::reconcile(paths, docker, runner).await;
            reconnect_or_create_session_with_focus(
                paths,
                container_name,
                focus_session,
                docker,
                runner,
            )
            .await
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

    finalize_reconnected_foreground_session(paths, container_name, docker, runner).await
}

async fn finalize_reconnected_foreground_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl crate::docker_client::DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let mut outcome =
        crate::runtime::launch::inspect_attach_outcome(docker, container_name).await?;
    super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let mut prompt = crate::isolation::finalize::StdinPrompt;
    let mut decision = crate::isolation::finalize::finalize_foreground_session(
        container_name,
        &paths.data_dir.join(container_name),
        outcome,
        interactive,
        &mut prompt,
        docker,
        runner,
    )
    .await?;

    if matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::ReturnToAgent
    ) {
        start_or_reconnect_capsule_client(paths, container_name, docker, runner).await?;
        outcome = crate::runtime::launch::inspect_attach_outcome(docker, container_name).await?;
        super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
        decision = crate::isolation::finalize::finalize_foreground_session(
            container_name,
            &paths.data_dir.join(container_name),
            outcome,
            interactive,
            &mut prompt,
            docker,
            runner,
        )
        .await?;
    }

    finalize_reconnected_resources(paths, container_name, outcome, decision, docker).await
}

async fn finalize_reconnected_resources(
    paths: &JackinPaths,
    container_name: &str,
    outcome: crate::isolation::finalize::AttachOutcome,
    decision: crate::isolation::finalize::FinalizeDecision,
    docker: &impl crate::docker_client::DockerApi,
) -> anyhow::Result<()> {
    use crate::isolation::finalize::{AttachOutcome, FinalizeDecision};

    let should_teardown = match (outcome, decision) {
        (_, FinalizeDecision::ReturnToAgent) => false,
        (AttachOutcome::Stopped(0), _)
        | (AttachOutcome::StillRunning, FinalizeDecision::Cleaned) => true,
        _ => false,
    };
    if !should_teardown {
        return Ok(());
    }

    let state_dir = paths.data_dir.join(container_name);
    let status = if matches!(decision, FinalizeDecision::Preserved) {
        super::launch::preserved_instance_status(&state_dir)?
    } else {
        crate::instance::InstanceStatus::CleanExited
    };
    if let Some(mut manifest) =
        crate::instance::InstanceManifest::read_or_log(&state_dir, "finalize_reconnected_resources")
    {
        super::launch::write_instance_status(paths, &state_dir, &mut manifest, status)?;
    }
    super::cleanup::eject_role(paths, container_name, docker).await
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
            .map(|session| session.name.as_str())
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
    match crate::isolation::state::MountSummary::for_state_dir(state_dir) {
        Ok(summary) => summary.inspect_label(),
        Err(e) => format!("unknown (error reading state: {e})"),
    }
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

    // Shared spinner helper: it suppresses its own stderr output while the
    // rich launch cockpit owns the screen, so the sidecar stage shows only
    // in the rail rather than streaming "Waiting for ..." over the frame.
    crate::tui::prompt::spin_wait(
        "Waiting for Docker-in-Docker to be ready",
        MAX_ATTEMPTS,
        INTERVAL,
        || async {
            docker
                .exec_capture(dind_name, &["docker", "info"])
                .await
                .map(|_| ())
        },
    )
    .await
    .with_context(|| format!("timed out waiting for Docker-in-Docker sidecar {dind_name}"))?;

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
    use std::collections::{HashMap, VecDeque};

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
    async fn wait_for_capsule_daemon_polls_socket_status_command() {
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
                "Sessions: 1\n".to_string()
            ])),
            ..Default::default()
        };

        wait_for_capsule_daemon("jk-agent-smith", &docker)
            .await
            .unwrap();

        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|call| call.contains(&format!("sh -c {JACKIN_STATUS_CMD}"))),
            "expected socket/status wait command; recorded: {recorded:?}"
        );
    }

    #[tokio::test]
    async fn start_or_reconnect_uses_capsule_client_not_start_attach() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(VecDeque::from([ContainerState::Stopped {
                exit_code: 0,
                oom_killed: false,
            }])),
            exec_capture_queue: std::cell::RefCell::new(VecDeque::from([
                "Sessions: 1\n".to_string()
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        start_or_reconnect_capsule_client(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap();

        let docker_recorded = docker.recorded.borrow();
        assert!(
            docker_recorded
                .iter()
                .any(|call| call == "start_container:jk-agent-smith"),
            "expected detached Docker API start; recorded: {docker_recorded:?}"
        );
        assert!(
            docker_recorded
                .iter()
                .any(|call| call.contains(&format!("sh -c {JACKIN_STATUS_CMD}"))),
            "expected socket/status wait before client exec; recorded: {docker_recorded:?}"
        );
        assert!(
            runner.recorded.iter().any(|call| {
                call.contains("docker exec -it")
                    && call.contains("jk-agent-smith")
                    && call.contains("/jackin/runtime/jackin-capsule")
            }),
            "expected capsule client exec; recorded: {:?}",
            runner.recorded
        );
        assert!(
            !runner
                .recorded
                .iter()
                .any(|call| call.contains("docker start -ai")),
            "restart path must not attach to PID 1; recorded: {:?}",
            runner.recorded
        );
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
                    && c.contains("jk-agent-smith")
                    && c.contains("jackin-capsule")
            }),
            "expected jackin-capsule exec in recorded commands; got: {:?}",
            runner.recorded
        );
    }

    #[tokio::test]
    async fn hardline_clean_exit_ejects_runtime_resources() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
                ContainerState::Stopped {
                    exit_code: 0,
                    oom_killed: false,
                },
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap();

        let recorded = docker.recorded.borrow();
        assert!(
            recorded
                .iter()
                .any(|op| op == "docker rm -f jk-agent-smith"),
            "clean exit should remove role container; recorded: {recorded:?}"
        );
        assert!(
            recorded
                .iter()
                .any(|op| op == "docker rm -f jk-agent-smith-dind"),
            "clean exit should remove DinD sidecar; recorded: {recorded:?}"
        );
        assert!(
            recorded
                .iter()
                .any(|op| op == "docker volume rm jk-agent-smith-dind-certs"),
            "clean exit should remove cert volume; recorded: {recorded:?}"
        );
        assert!(
            recorded
                .iter()
                .any(|op| op == "docker network rm jk-agent-smith-net"),
            "clean exit should remove role network; recorded: {recorded:?}"
        );
    }

    #[tokio::test]
    async fn hardline_detach_with_live_sessions_preserves_runtime_resources() {
        let (_tmp, paths) = test_paths();
        let docker = FakeDockerClient {
            inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                ContainerState::Running,
                ContainerState::Running,
            ])),
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                "Sessions: 1\n  [1] Claude (claude) state=working active=true".to_string(),
            ])),
            ..Default::default()
        };
        let mut runner = FakeRunner::default();

        hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
            .await
            .unwrap();

        assert!(
            !docker
                .recorded
                .borrow()
                .iter()
                .any(|op| op.starts_with("docker rm -f")),
            "detach with live sessions must not eject resources; recorded: {:?}",
            docker.recorded.borrow()
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
                    && !call.contains("JACKIN_AGENT=")
                    && call.contains("--workdir /workspace/project")
                    && call.contains("jk-k7p9m2xq-workspace-agentsmith")
                    && call.contains("jackin-capsule")
                    && call.contains("new")
                    && call.contains("codex")
            }),
            "expected jackin-capsule new for codex; got: {:?}",
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
                .any(|call| call.contains("-e=JACKIN_GIT_COAUTHOR_TRAILER=1")),
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
                .any(|call| call.contains("-e=JACKIN_GIT_DCO=1")),
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
    async fn spawn_shell_session_execs_jackin_capsule_new_in_running_container() {
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
                    && c.contains("jk-agent-smith")
                    && c.contains("jackin-capsule")
                    && c.contains("new")
            }),
            "expected docker exec with jackin-capsule new; got: {:?}",
            runner.recorded
        );
    }

    #[tokio::test]
    async fn spawn_shell_session_does_not_set_tmux_env() {
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
            !runner.recorded.iter().any(|c| c.contains("TMUX=")),
            "TMUX= must not be set in jackin-capsule shell sessions"
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
                .any(|c| c.contains("docker start") || c.contains("jackin-capsule new"))
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
                .any(|c| c.contains("docker start") || c.contains("jackin-capsule new"))
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
        // exec_capture: jackin-capsule status returns two sessions
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
                "Sessions: 2\n  [1] jackin-claude-abc123 (claude) state=working active=true\n  [2] jackin-codex-abc (codex) state=idle active=false".to_string(),
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
    async fn inspect_agent_sessions_lists_jackin_sessions() {
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                "Sessions: 2\n  [1] Claude (claude) state=working active=true\n  [2] Codex (codex) state=idle active=false".to_string(),
            ])),
            ..Default::default()
        };

        let sessions =
            inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

        let AgentSessionInventory::Sessions(sessions) = sessions else {
            panic!("expected sessions");
        };
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].name, "Claude");
        assert_eq!(sessions[1].name, "Codex");
    }

    #[tokio::test]
    async fn inspect_agent_sessions_returns_empty_when_no_sessions_running() {
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                "Sessions: 0".to_string(),
            ])),
            ..Default::default()
        };

        let sessions =
            inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

        assert_eq!(sessions, AgentSessionInventory::Sessions(vec![]));
    }

    #[tokio::test]
    async fn inspect_agent_sessions_returns_unavailable_on_missing_header() {
        // A daemon that crashed mid-call or a cosmetic change to the
        // status print must surface as Unavailable, not as "zero sessions".
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                String::new(),
            ])),
            ..Default::default()
        };

        let sessions =
            inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

        assert!(
            matches!(sessions, AgentSessionInventory::Unavailable(_)),
            "expected Unavailable on missing header; got {sessions:?}"
        );
    }

    #[tokio::test]
    async fn inspect_agent_sessions_returns_unavailable_on_count_mismatch() {
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                "Sessions: 5\n  [1] Claude (claude) state=working active=true".to_string(),
            ])),
            ..Default::default()
        };

        let sessions =
            inspect_agent_sessions(&docker, "jk-agent-smith", &ContainerState::Running).await;

        assert!(
            matches!(sessions, AgentSessionInventory::Unavailable(_)),
            "expected Unavailable on count mismatch; got {sessions:?}"
        );
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
                .any(|c| c.contains("docker start") || c.contains("jackin-capsule new"))
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

    #[tokio::test]
    async fn spawn_shell_session_succeeds_when_container_paused_or_restarting() {
        for state in [ContainerState::Paused, ContainerState::Restarting] {
            let (_tmp, paths) = test_paths();
            let docker = FakeDockerClient {
                inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                    state.clone()
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
                        && c.contains("jk-agent-smith")
                        && c.contains("jackin-capsule")
                }),
                "state={state:?}: expected docker exec with jackin-capsule; got: {:?}",
                runner.recorded
            );
        }
    }

    #[tokio::test]
    async fn hardline_agent_errors_on_inactive_states() {
        let cases: &[(ContainerState, &str)] = &[
            (ContainerState::Created, "created"),
            (ContainerState::Dead, "dead"),
            (ContainerState::Removing, "removing"),
        ];
        for (state, expected_phrase) in cases {
            let (_tmp, paths) = test_paths();
            let docker = FakeDockerClient {
                inspect_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                    state.clone()
                ])),
                ..Default::default()
            };
            let mut runner = FakeRunner::default();
            let err = hardline_agent(&paths, "jk-agent-smith", &docker, &mut runner)
                .await
                .unwrap_err();
            assert!(
                err.to_string().contains(expected_phrase),
                "state={state:?}: expected phrase {expected_phrase:?}; got: {err}"
            );
            assert!(
                !runner
                    .recorded
                    .iter()
                    .any(|c| c.contains("tmux") || c.contains("docker start")),
                "state={state:?}: no exec or start must fire"
            );
        }
    }

    #[tokio::test]
    async fn inspect_agent_sessions_returns_not_running_for_non_running_states() {
        for state in [ContainerState::Paused, ContainerState::Restarting] {
            let docker = FakeDockerClient::default();
            let sessions = inspect_agent_sessions(&docker, "jk-agent-smith", &state).await;
            assert_eq!(
                sessions,
                AgentSessionInventory::NotRunning,
                "state={state:?}"
            );
            assert!(
                docker.recorded.borrow().is_empty(),
                "state={state:?}: exec_capture must not be called"
            );
        }
    }

    #[tokio::test]
    async fn wait_for_dind_succeeds_when_daemon_ready_immediately() {
        // docker info succeeds on first attempt; test -f /certs/client/ca.pem also succeeds.
        let docker = FakeDockerClient {
            exec_capture_queue: std::cell::RefCell::new(std::collections::VecDeque::from([
                String::new(), // docker info
                String::new(), // test -f /certs/client/ca.pem
            ])),
            ..Default::default()
        };

        wait_for_dind("jk-agent-smith-dind", "jk-agent-smith-dind-certs", &docker)
            .await
            .unwrap();
    }
}
