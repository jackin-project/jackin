// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Session attach/reconnect/hardline for running containers.
//!
//! Drives capsule client connections and session inventory queries against a
//! live container's daemon socket. Not responsible for container start-up,
//! image build, or identity resolution — those live in sibling modules.
//!
//! Key invariant: callers treat `AgentSessionInventory::Unavailable` as a
//! transient state during the setup-once window; they must not surface it as
//! a terminal error.

#![expect(
    clippy::print_stderr,
    reason = "attach flow emits intentional terminal spacing on stderr"
)]

use crate::instance::{InstanceIndex, InstanceManifest, InstanceStatus, RegistrationState};
use anyhow::Context as _;
use jackin_core::container_paths;
use jackin_core::{CommandRunner, ContainerHandle, JACKIN_STATUS_CMD, RunOptions};
use jackin_docker::docker_client::DockerApi;
use jackin_protocol::attach::SpawnRequest;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct ReconnectAdmissionFailure(String);

impl std::fmt::Display for ReconnectAdmissionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "reconnect admission failed: {}", self.0)
    }
}

impl std::error::Error for ReconnectAdmissionFailure {}

fn mark_reconnect_admission_failure(error: anyhow::Error) -> anyhow::Error {
    let summary = error.to_string();
    error.context(ReconnectAdmissionFailure(summary))
}

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
pub const JACKIN_CAPSULE_PATH: &str = container_paths::CAPSULE_BIN;
pub const ATTACH_PROXY_SUBCOMMAND: &str = "attach-proxy";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostAttachTransportPlan {
    DirectSocket {
        socket_path: PathBuf,
    },
    AttachProxy {
        socket_path: PathBuf,
        direct_error: Option<String>,
    },
}

pub fn attach_proxy_exec_args(container: &ContainerHandle) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "-i".to_owned(),
        container.id().to_owned(),
        JACKIN_CAPSULE_PATH.to_owned(),
        ATTACH_PROXY_SUBCOMMAND.to_owned(),
    ]
}

/// Conservative `sockaddr_un.sun_path` capacity across the platforms jackin'
/// targets (macOS/BSD = 104, Linux = 108). A socket path at or above this cannot
/// be `connect`ed directly — the kernel rejects it — so the direct transport is
/// impossible regardless of whether the socket exists.
pub(crate) const MAX_UNIX_SOCKET_PATH_LEN: usize = 104;

pub fn select_host_attach_transport(
    paths: &JackinPaths,
    container_name: &str,
) -> HostAttachTransportPlan {
    let socket_path = super::snapshot::socket_path(paths, container_name);

    // A path at/over the `sun_path` limit can never bind/connect directly; the OS
    // returns a generic error that reads like "connection refused", silently
    // degrading to the attach-proxy and conflating "too long" with "not ready"
    // (Bug 10). Detect it explicitly and surface it at a visible tier with a
    // precise reason, instead of leaving it to a swallowed connect error.
    let path_len = socket_path.as_os_str().len();
    if path_len >= MAX_UNIX_SOCKET_PATH_LEN {
        let reason = format!(
            "socket path is {path_len} bytes, at/over the {MAX_UNIX_SOCKET_PATH_LEN}-byte \
             sun_path limit; using attach-proxy (shorten the jackin state dir)"
        );
        let _warning = jackin_telemetry::record_recovered_degradation();
        return HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error: Some(reason),
        };
    }

    if !socket_path.exists() {
        return HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error: None,
        };
    }

    match jackin_diagnostics::operation::connection_attempt_sync(
        jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleAttach,
        || std::os::unix::net::UnixStream::connect(&socket_path),
    ) {
        Ok(_) => HostAttachTransportPlan::DirectSocket { socket_path },
        Err(err) => HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error: Some(err.to_string()),
        },
    }
}

async fn wait_for_capsule_daemon_with_handle(
    paths: &JackinPaths,
    container: &ContainerHandle,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
    const INITIAL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
    const MAX_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "wait_capsule_socket",
        Some(container.name()),
    );
    let wait_result = wait_for_capsule_daemon_ready(
        paths,
        container,
        docker,
        MAX_WAIT,
        INITIAL_INTERVAL,
        MAX_INTERVAL,
    )
    .await
    .with_context(|| format!("waiting for jackin-capsule daemon in {}", container.name()));
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "wait_capsule_socket",
        if wait_result.is_ok() {
            Some("ready")
        } else {
            Some("error")
        },
    );
    if wait_result.is_err() {
        let _error = jackin_telemetry::record_error(
            jackin_telemetry::schema::enums::ErrorType::LaunchFailed,
        );
        jackin_diagnostics::emit_operator_notice("container readiness wait failed");
    }
    wait_result
}

async fn wait_for_capsule_daemon_ready(
    paths: &JackinPaths,
    container: &ContainerHandle,
    docker: &impl DockerApi,
    max_wait: std::time::Duration,
    initial_interval: std::time::Duration,
    max_interval: std::time::Duration,
) -> anyhow::Result<()> {
    let started = tokio::time::Instant::now();
    let mut interval = initial_interval;

    loop {
        if capsule_daemon_socket_connects(paths, container.name()) {
            return Ok(());
        }

        let Err(exec_error) = docker
            .exec_capture_by_id(container, &["sh", "-c", JACKIN_STATUS_CMD])
            .await
        else {
            return Ok(());
        };

        if started.elapsed() >= max_wait {
            return Err(exec_error).with_context(|| {
                format!("timed out after {max_wait:?} waiting for capsule daemon readiness")
            });
        }

        tokio::time::sleep(interval).await;
        interval = (interval * 2).min(max_interval);
    }
}

fn capsule_daemon_socket_connects(paths: &JackinPaths, container_name: &str) -> bool {
    let socket_path = super::snapshot::socket_path(paths, container_name);
    socket_path.exists()
        && jackin_diagnostics::operation::connection_attempt_sync(
            jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleAttach,
            || std::os::unix::net::UnixStream::connect(socket_path),
        )
        .is_ok()
}

use jackin_core::JackinPaths;
pub use jackin_docker::docker_client::ContainerState;

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
    container: &ContainerHandle,
    state: &ContainerState,
) -> AgentSessionInventory {
    if matches!(state, ContainerState::InspectUnavailable(_)) {
        return AgentSessionInventory::Unavailable(
            "container state unavailable; skipping session query".to_owned(),
        );
    }
    if !matches!(state, ContainerState::Running) {
        return AgentSessionInventory::NotRunning;
    }

    match docker
        .exec_capture_by_id(container, &["sh", "-c", JACKIN_STATUS_CMD])
        .await
    {
        Ok(output) => match parse_jackin_sessions(&output) {
            Ok(sessions) => AgentSessionInventory::Sessions(sessions),
            Err(reason) => AgentSessionInventory::Unavailable(reason),
        },
        Err(error) => AgentSessionInventory::Unavailable(error.to_string()),
    }
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
    let expected = jackin_core::parse_session_count(output).ok_or_else(|| {
        "jackin-capsule status emitted no parsable `Sessions: N` header — daemon may be unreachable".to_owned()
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
                name: name.to_owned(),
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
    let title = if let Ok(manifest) = InstanceManifest::read(&paths.data_dir.join(container_name)) {
        manifest.role_display_name
    } else {
        let _warning = jackin_telemetry::record_recovered_degradation();
        container_name.to_owned()
    };
    jackin_diagnostics::set_terminal_title(&title);
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
    jackin_diagnostics::host_screen_owned().then_some("-e=JACKIN_HOST_ALT_SCREEN=1")
}

/// Insert the root-supervisor identity right after `exec`. Attach/control
/// commands talk to the root-owned capsule socket and must not fall back to
/// the image's baked `agent` UID or a host-operator UID shared with sessions.
fn insert_run_as_user<'a>(args: &mut Vec<&'a str>, run_as_user: Option<&'a str>) {
    if let Some(user) = run_as_user {
        args.insert(1, user);
        args.insert(1, "--user");
    }
}

/// Git policy toggles as `(ENV_NAME, "1")` pairs — the single source of truth for
/// which toggle gates which env var. The host-attach and docker-exec transports
/// each adapt these pairs to their own wire shape (`SpawnRequest` env tuples vs
/// `-e=NAME=1` flags).
fn git_policy_env_pairs(coauthor_trailer: bool, dco: bool) -> Vec<(&'static str, &'static str)> {
    let mut pairs = Vec::with_capacity(2);
    if coauthor_trailer {
        pairs.push((jackin_core::JACKIN_GIT_COAUTHOR_TRAILER_ENV_NAME, "1"));
    }
    if dco {
        pairs.push((jackin_core::JACKIN_GIT_DCO_ENV_NAME, "1"));
    }
    pairs
}

/// Existing containers retain credential material: every attach route must
/// recheck their recorded admission against current host policy before use.
pub(crate) fn require_current_account_admission(
    paths: &JackinPaths,
    container_name: &str,
) -> anyhow::Result<super::launch::AccountConfigRevision> {
    let admission_lease = super::launch::AccountConfigRevision::acquire(paths)?;
    validate_current_account_admission(paths, container_name, &admission_lease)?;
    Ok(admission_lease)
}

pub(crate) fn validate_current_account_admission(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
) -> anyhow::Result<()> {
    let root = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read(&root)
        .context("cannot verify this container's account policy; recreate it with `jackin load`")?;
    let manifest = refresh_registration_states(paths, &root, manifest)?;
    current_account_admission(paths, &root, &manifest)?;
    admission_lease.ensure_current(paths)?;
    Ok(())
}

fn refresh_registration_states(
    paths: &JackinPaths,
    root: &std::path::Path,
    mut manifest: InstanceManifest,
) -> anyhow::Result<InstanceManifest> {
    let snapshot = jackin_config::load_read_only_config_snapshot(paths)?;
    if !snapshot.diagnostics.is_empty() {
        return Ok(manifest);
    }
    let workspace = manifest
        .workspace_name
        .as_deref()
        .map(jackin_core::WorkspaceName::parse)
        .transpose()?;
    let mut changed = false;
    for admitted in manifest.admitted_instances.clone() {
        let state =
            registration_state_for_admission(&snapshot.config, workspace.as_ref(), &admitted);
        changed |= manifest.mark_registration_state(&admitted.config_id, state);
    }
    if changed {
        manifest.touch();
        manifest.write(root)?;
        InstanceIndex::update_manifest(&paths.data_dir, &manifest)?;
    }
    Ok(manifest)
}

fn registration_state_for_admission(
    config: &jackin_config::AppConfig,
    workspace: Option<&jackin_core::WorkspaceName>,
    admitted: &crate::instance::AdmittedInstance,
) -> RegistrationState {
    let Some(account) = config.accounts.get(&admitted.account_id) else {
        return RegistrationState::Removed;
    };
    if !account.enabled || !account.supports_agent(admitted.agent) {
        return RegistrationState::Disabled;
    }
    if workspace.is_some_and(|workspace| {
        !config
            .workspaces
            .get(workspace.as_str())
            .is_some_and(|workspace| workspace.accounts.contains(&admitted.account_id))
    }) {
        return RegistrationState::Disabled;
    }
    match config.agent_configurations.get(&admitted.config_id) {
        Some(configuration)
            if configuration.agent == admitted.agent
                && configuration.account == admitted.account_id =>
        {
            RegistrationState::Current
        }
        Some(_) => RegistrationState::Removed,
        None if admitted.config_id
            == format!("{}@{}", admitted.account_id, admitted.agent.slug()) =>
        {
            RegistrationState::Current
        }
        None => RegistrationState::Removed,
    }
}

fn current_account_admission(
    paths: &JackinPaths,
    root: &std::path::Path,
    manifest: &InstanceManifest,
) -> anyhow::Result<(jackin_config::AppConfig, Option<jackin_core::WorkspaceName>)> {
    let snapshot = jackin_config::load_read_only_config_snapshot(paths)
        .context("cannot read current account policy")?;
    anyhow::ensure!(
        snapshot.diagnostics.is_empty(),
        "current account configuration is unavailable or invalid; reconnect denied"
    );
    let workspace = manifest
        .workspace_name
        .as_deref()
        .map(jackin_core::WorkspaceName::parse)
        .transpose()?;
    anyhow::ensure!(
        super::account_admission_matches(
            root,
            &snapshot.config,
            workspace.as_ref(),
            &manifest.role_key
        )?,
        "container account policy changed or cannot be verified; recreate it with `jackin load`"
    );
    Ok((snapshot.config, workspace))
}

/// Revalidate a requested new-session target against the manifest captured by
/// the live container and the current host account policy. Every v3 manifest
/// must carry an explicit admission set; the function never resolves an
/// account by provider/name and never silently substitutes an unqualified
/// same-agent row.
fn require_current_instance_admission(
    paths: &JackinPaths,
    container_name: &str,
    agent: jackin_core::Agent,
    requested_instance_id: Option<&str>,
    admission_lease: &super::launch::AccountConfigRevision,
) -> anyhow::Result<(InstanceManifest, Option<String>)> {
    let root = paths.data_dir.join(container_name);
    let manifest = InstanceManifest::read(&root).context(
        "cannot verify this container's live instance admission; recreate it with `jackin load`",
    )?;
    let manifest = refresh_registration_states(paths, &root, manifest)?;

    let target = if let Some(requested) = requested_instance_id {
        manifest
            .admitted_instances
            .iter()
            .find(|admitted| admitted.config_id == requested)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "instance {requested:?} is not admitted by the live container manifest"
                )
            })?
    } else {
        let mut matches = manifest
            .admitted_instances
            .iter()
            .filter(|admitted| admitted.agent == agent);
        let Some(target) = matches.next() else {
            anyhow::bail!("agent {agent} is not admitted by the live container manifest");
        };
        anyhow::ensure!(
            matches.next().is_none(),
            "agent {agent} has multiple live instances; select an exact instance ID"
        );
        target
    };
    anyhow::ensure!(
        target.agent == agent,
        "instance {:?} is admitted for {}, not {}",
        target.config_id,
        target.agent,
        agent
    );
    let target_id = target.config_id.clone();

    if target.registration_state != RegistrationState::Current {
        anyhow::bail!(
            "container account policy changed: admitted account {:?} registration is {}; stop and recreate the instance before requesting a new session",
            target.account_id,
            target.registration_state.label()
        );
    }

    let (config, workspace) = current_account_admission(paths, &root, &manifest)?;

    let account = config.accounts.get(&target.account_id).ok_or_else(|| {
        anyhow::anyhow!(
            "admitted account {:?} is no longer registered",
            target.account_id
        )
    })?;
    anyhow::ensure!(
        account.enabled && account.supports_agent(agent),
        "admitted account {:?} no longer authorizes {}",
        target.account_id,
        agent
    );
    if let Some(workspace) = workspace.as_ref() {
        anyhow::ensure!(
            config
                .workspaces
                .get(workspace.as_str())
                .is_some_and(|workspace| workspace.accounts.contains(&target.account_id)),
            "admitted account {:?} is no longer assigned to workspace {:?}",
            target.account_id,
            workspace
        );
    }
    if let Some(configuration) = config.agent_configurations.get(&target_id) {
        anyhow::ensure!(
            configuration.agent == agent && configuration.account == target.account_id,
            "live instance {target_id:?} no longer matches the current agent/account configuration"
        );
    } else {
        anyhow::ensure!(
            target_id == format!("{}@{}", target.account_id, agent.slug()),
            "live instance {target_id:?} no longer exists in the current account configuration"
        );
    }

    // The capsule receives the same exact ID and performs the final immutable
    // launch-config admission check before creating the PTY.
    admission_lease.ensure_current(paths)?;
    Ok((manifest, Some(target_id)))
}

pub(super) async fn reconnect_or_create_session_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let admission_lease = require_current_account_admission(paths, container_name)
        .map_err(mark_reconnect_admission_failure)?;
    reconnect_or_create_session_with_focus_with_lease(
        paths,
        container_name,
        focus_session,
        &admission_lease,
        docker,
        runner,
    )
    .await
}

pub(super) async fn reconnect_or_create_session_with_focus_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)
        .map_err(mark_reconnect_admission_failure)?;
    let inspection = docker.inspect_container_by_name(container_name).await;
    let container = inspection
        .handle
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot resolve container {container_name}: {}",
                inspection.state.inspect_label()
            )
        })
        .map_err(mark_reconnect_admission_failure)?;
    reconnect_or_create_session_with_container_handle_with_lease(
        paths,
        container_name,
        focus_session,
        admission_lease,
        docker,
        runner,
        &container,
    )
    .await
}

pub(super) async fn reconnect_or_create_session_with_container_handle_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
) -> anyhow::Result<()> {
    set_role_terminal_title(paths, container_name);
    wait_for_capsule_daemon_with_handle(paths, container, docker)
        .await
        .map_err(mark_reconnect_admission_failure)?;
    admission_lease
        .ensure_current(paths)
        .map_err(mark_reconnect_admission_failure)?;
    if super::host_attach::host_attach_enabled(paths) {
        let outcome =
            super::host_attach::run_host_attach_session(paths, container, None, focus_session, &[])
                .await;
        jackin_diagnostics::reassert_alt_screen();
        admission_lease
            .ensure_current(paths)
            .map_err(mark_reconnect_admission_failure)?;
        return outcome;
    }
    let focus_arg = focus_session.map(|id| id.to_string());
    let run_as_user = Some(crate::runtime::identity::CAPSULE_SUPERVISOR_USER);
    let mut args: Vec<&str> = vec!["exec", "-it", container.id(), container_paths::CAPSULE_BIN];
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    insert_run_as_user(&mut args, run_as_user);
    if let Some(ref id) = focus_arg {
        args.push("--focus");
        args.push(id);
    }
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "capsule_client_exec",
        Some(container_name),
    );
    admission_lease
        .ensure_current(paths)
        .map_err(mark_reconnect_admission_failure)?;
    let outcome = runner
        .run(
            "docker",
            &args,
            None,
            &RunOptions {
                interactive: true,
                ..RunOptions::default()
            },
        )
        .await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "capsule_client_exec",
        if outcome.is_ok() {
            Some("detached")
        } else {
            Some("error")
        },
    );
    if outcome.is_ok()
        && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            jackin_telemetry::schema::events::CAPSULE_SESSION_DETACH,
            "operator detached from capsule session",
        );
    }
    // The capsule has detached; re-claim the alt screen before any post-attach
    // work so the exit flow does not flash the operator's shell.
    jackin_diagnostics::reassert_alt_screen();
    admission_lease
        .ensure_current(paths)
        .map_err(mark_reconnect_admission_failure)?;
    outcome
}

pub(super) async fn start_or_reconnect_capsule_client(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let admission_lease = require_current_account_admission(paths, container_name)?;
    start_or_reconnect_capsule_client_with_lease(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
    )
    .await
}

pub(super) async fn start_or_reconnect_capsule_client_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    start_or_reconnect_capsule_client_with_handle_with_lease(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        None,
    )
    .await
    .map(|_| ())
}

async fn start_or_reconnect_capsule_client_with_handle_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    known_container: Option<&ContainerHandle>,
) -> anyhow::Result<ContainerHandle> {
    validate_current_account_admission(paths, container_name, admission_lease)?;
    if let Some(container) = known_container {
        anyhow::ensure!(
            container.name() == container_name,
            "container handle name mismatch: expected {container_name}, got {}",
            container.name()
        );
    }
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "restore_inspect",
        Some(container_name),
    );
    let (inspect, inspect_handle) = if let Some(container) = known_container {
        (
            docker.inspect_container_by_id(container).await,
            Some(container.clone()),
        )
    } else {
        let inspection = docker.inspect_container_by_name(container_name).await;
        (inspection.state, inspection.handle)
    };
    let inspect_label = inspect.short_label();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Capsule,
        "restore_inspect",
        Some(&inspect_label),
    );
    match inspect {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            if inspect_handle.is_none() {
                anyhow::bail!("container '{container_name}' inspection returned no immutable ID");
            }
        }
        ContainerState::Stopped { .. } | ContainerState::Created => {
            let Some(container) = inspect_handle.clone() else {
                anyhow::bail!("container '{container_name}' inspection returned no immutable ID");
            };
            let resources =
                crate::runtime::cleanup::docker_resources_for_state(paths, container_name);
            restart_stopped_dind_if_needed(paths, container_name, admission_lease, docker).await?;

            jackin_diagnostics::active_timing_started(
                jackin_diagnostics::DiagnosticStage::Capsule,
                "restore_start_container",
                Some(container_name),
            );
            admission_lease.ensure_current(paths)?;
            let start_result = docker
                .start_container_by_id(&container)
                .await
                .with_context(|| format!("starting role container {container_name}"));
            jackin_diagnostics::active_timing_done(
                jackin_diagnostics::DiagnosticStage::Capsule,
                "restore_start_container",
                if start_result.is_ok() {
                    Some("started")
                } else {
                    Some("error")
                },
            );
            if let Err(start_err) = start_result {
                super::launch::ensure_current_or_remove_stale_container(
                    admission_lease,
                    paths,
                    &container,
                    docker,
                )
                .await?;
                let net_missing = if let Ok(None) = docker.inspect_network(&resources.network).await
                {
                    true
                } else {
                    let err_msg = start_err.to_string();
                    err_msg.contains("network")
                        && (err_msg.contains("not found") || err_msg.contains("404"))
                };
                super::launch::ensure_current_or_remove_stale_container(
                    admission_lease,
                    paths,
                    &container,
                    docker,
                )
                .await?;
                if net_missing {
                    anyhow::bail!(
                        "role container '{container_name}' cannot be started because its Docker network '{}' no longer exists; \
                         run `jackin load` to recreate the instance, or `jackin eject {container_name}` to discard it",
                        resources.network
                    );
                }
                return Err(start_err);
            }
            super::launch::ensure_current_or_remove_stale_container(
                admission_lease,
                paths,
                &container,
                docker,
            )
            .await?;
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
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    let Some(container) = inspect_handle else {
        anyhow::bail!("container '{container_name}' has no immutable ID for attach");
    };
    reconnect_or_create_session_with_container_handle_with_lease(
        paths,
        container_name,
        None,
        admission_lease,
        docker,
        runner,
        &container,
    )
    .await?;
    Ok(container)
}

async fn restart_stopped_dind_if_needed(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    let resources = crate::runtime::cleanup::docker_resources_for_state(paths, container_name);
    if resources.dind_container.is_none() {
        return Ok(());
    }
    let Some(dind) =
        crate::runtime::cleanup::resolve_dind_handle_for_state(paths, container_name, docker)
            .await?
    else {
        return Ok(());
    };
    let dind_state = docker.inspect_container_by_id(&dind).await;
    if !matches!(
        dind_state,
        ContainerState::Stopped { .. } | ContainerState::Created
    ) {
        return Ok(());
    }
    admission_lease.ensure_current(paths)?;
    drop(docker.start_container_by_id(&dind).await);
    super::launch::ensure_current_or_remove_stale_container(admission_lease, paths, &dind, docker)
        .await
}

pub(super) async fn start_or_hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    start_first: bool,
) -> anyhow::Result<()> {
    start_or_hardline_agent_with_known_container(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        start_first,
        None,
    )
    .await
}

pub(super) async fn start_or_hardline_agent_with_container_handle(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    start_first: bool,
    container: &ContainerHandle,
) -> anyhow::Result<()> {
    start_or_hardline_agent_with_known_container(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        start_first,
        Some(container),
    )
    .await
}

async fn start_or_hardline_agent_with_known_container(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    start_first: bool,
    known_container: Option<&ContainerHandle>,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)?;
    match crate::runtime::backend::backend_for_state(paths, container_name) {
        crate::runtime::backend::InstanceBackend::Docker => {
            if start_first {
                let container = start_or_reconnect_capsule_client_with_handle_with_lease(
                    paths,
                    container_name,
                    admission_lease,
                    docker,
                    runner,
                    known_container,
                )
                .await?;
                admission_lease.ensure_current(paths)?;
                finalize_reconnected_foreground_session_with_handle(
                    paths,
                    container_name,
                    admission_lease,
                    docker,
                    runner,
                    &container,
                )
                .await
            } else {
                let container = if let Some(container) = known_container {
                    container.clone()
                } else {
                    let inspection = docker.inspect_container_by_name(container_name).await;
                    inspection.handle.ok_or_else(|| {
                        anyhow::anyhow!(
                            "cannot resolve container {container_name}: {}",
                            inspection.state.inspect_label()
                        )
                    })?
                };
                hardline_docker_agent_with_focus_with_lease(
                    paths,
                    container_name,
                    None,
                    admission_lease,
                    docker,
                    runner,
                    &container,
                )
                .await
            }
        }
        crate::runtime::backend::InstanceBackend::AppleContainer => {
            anyhow::ensure!(
                known_container.is_none(),
                "immutable Docker container handle supplied for Apple Container backend"
            );
            admission_lease.ensure_current(paths)?;
            crate::runtime::apple_container::reconnect(paths, container_name, None).await?;
            admission_lease.ensure_current(paths)?;
            anyhow::bail!("apple-container finalize not yet implemented - Phase 0")
        }
    }
}

/// Verify the container is reachable (running/paused/restarting).
/// Returns `Ok(())` when reachable, `Err` otherwise.
/// `stopped_hint` is the trailing clause of the "is stopped" error, e.g. "restart it before opening a shell".
async fn require_container_reachable(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    stopped_hint: &str,
) -> anyhow::Result<(super::launch::AccountConfigRevision, ContainerHandle)> {
    let container = require_container_running(paths, container_name, docker, stopped_hint).await?;
    let admission_lease = super::launch::AccountConfigRevision::acquire(paths)?;
    validate_current_account_admission(paths, container_name, &admission_lease)?;
    Ok((admission_lease, container))
}

/// Verify only the Docker lifecycle state. New-agent sessions call this
/// before their single fresh manifest/policy admission gate so that target
/// selection and revalidation remain one pre-exec decision.
async fn require_container_running(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    stopped_hint: &str,
) -> anyhow::Result<ContainerHandle> {
    let inspection = docker.inspect_container_by_name(container_name).await;
    match inspection.state {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            inspection.handle.ok_or_else(|| {
                anyhow::anyhow!("container '{container_name}' inspection returned no immutable ID")
            })
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
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let (admission_lease, container) = require_container_reachable(
        paths,
        container_name,
        docker,
        "restart it before opening a shell",
    )
    .await?;
    set_role_terminal_title(paths, container_name);
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    admission_lease.ensure_current(paths)?;
    if super::host_attach::host_attach_enabled(paths) {
        let result = super::host_attach::run_host_attach_session(
            paths,
            &container,
            Some(SpawnRequest::Shell),
            None,
            &[],
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        admission_lease.ensure_current(paths)?;
        eprintln!();
        result?;
        return finalize_reconnected_foreground_session_with_handle(
            paths,
            container_name,
            &admission_lease,
            docker,
            runner,
            &container,
        )
        .await;
    }
    let run_as_user = Some(crate::runtime::identity::CAPSULE_SUPERVISOR_USER);
    let mut args: Vec<&str> = vec![
        "exec",
        "-it",
        container.id(),
        container_paths::CAPSULE_BIN,
        "new",
    ];
    insert_run_as_user(&mut args, run_as_user);
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "shell_session_exec",
        Some(container_name),
    );
    admission_lease.ensure_current(paths)?;
    let result = runner
        .run(
            "docker",
            &args,
            None,
            &RunOptions {
                interactive: true,
                ..RunOptions::default()
            },
        )
        .await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "shell_session_exec",
        if result.is_ok() {
            Some("detached")
        } else {
            Some("error")
        },
    );
    if result.is_ok()
        && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            jackin_telemetry::schema::events::CAPSULE_SESSION_DETACH,
            "operator detached from shell session",
        );
    }
    jackin_diagnostics::reassert_alt_screen();
    admission_lease.ensure_current(paths)?;
    eprintln!();
    result?;
    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
        &container,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "Spawning a single agent session requires every caller-supplied \
              parameter (paths, container_name, requested_instance_id, agent, \
              env_overrides, git config, docker, runner, ...) to flow through to \
              the container bring-up path; bundling into a config struct would be \
              a parallel pass that requires restructuring the spawn path. Named- \
              arg reads match the per-input propagation idiom."
)]
pub async fn spawn_agent_session(
    paths: &JackinPaths,
    container_name: &str,
    requested_instance_id: Option<&str>,
    agent: jackin_core::Agent,
    env_overrides: &[(String, String)],
    git_coauthor_trailer: bool,
    git_dco: bool,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !env_overrides
            .iter()
            .any(|(name, _)| jackin_env::is_account_env(name)),
        "account credential and routing overrides are not allowed; select an assigned account and recreate the container"
    );
    let container = require_container_running(
        paths,
        container_name,
        docker,
        "restart or recover it before using `--new`",
    )
    .await?;
    let admission_lease = super::launch::AccountConfigRevision::acquire(paths)?;

    let (live_manifest, admitted_instance_id) = require_current_instance_admission(
        paths,
        container_name,
        agent,
        requested_instance_id,
        &admission_lease,
    )?;
    let workdir = live_manifest.workdir.as_str();
    let spawn_target = admitted_instance_id
        .as_deref()
        .unwrap_or_else(|| agent.slug());

    // Instance selection travels as `jackin-capsule new <instance-id>` argv; the
    // git policy toggles are session env consumed by the spawned entrypoint.
    // Each transport encodes them only on the path that consumes it.
    set_role_terminal_title(paths, container_name);
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    if super::host_attach::host_attach_enabled(paths) {
        let mut session_env_overrides: Vec<(String, String)> =
            git_policy_env_pairs(git_coauthor_trailer, git_dco)
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect();
        session_env_overrides.extend(env_overrides.iter().cloned());
        let spawn_request = SpawnRequest::instance(spawn_target)?;
        admission_lease.ensure_current(paths)?;
        let result = super::host_attach::run_host_attach_session(
            paths,
            &container,
            Some(spawn_request),
            None,
            &session_env_overrides,
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        admission_lease.ensure_current(paths)?;
        eprintln!();
        result?;
        return finalize_reconnected_foreground_session_with_handle(
            paths,
            container_name,
            &admission_lease,
            docker,
            runner,
            &container,
        )
        .await;
    }

    let run_as_user = Some(crate::runtime::identity::CAPSULE_SUPERVISOR_USER);
    let mut exec_args = vec!["exec", "--workdir", workdir, "-it"];
    insert_run_as_user(&mut exec_args, run_as_user);
    // Git policy and non-account session environment outlive `exec_args`.
    let env_flags: Vec<String> = git_policy_env_pairs(git_coauthor_trailer, git_dco)
        .into_iter()
        .map(|(name, value)| format!("-e={name}={value}"))
        .chain(env_overrides.iter().map(|(k, v)| format!("-e={k}={v}")))
        .collect();
    for flag in &env_flags {
        exec_args.push(flag.as_str());
    }
    exec_args.push(container.id());
    exec_args.extend_from_slice(&[container_paths::CAPSULE_BIN, "new", spawn_target]);
    if let Some(flag) = host_alt_screen_exec_flag() {
        exec_args.insert(1, flag);
    }
    let timing_name = format!("new_{}_session_exec", agent.slug());
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        &timing_name,
        Some(container_name),
    );
    admission_lease.ensure_current(paths)?;
    let result = runner
        .run(
            "docker",
            &exec_args,
            None,
            &RunOptions {
                interactive: true,
                ..RunOptions::default()
            },
        )
        .await;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        &timing_name,
        if result.is_ok() {
            Some("detached")
        } else {
            Some("error")
        },
    );
    if result.is_ok()
        && let Some(run) = jackin_diagnostics::active_run()
    {
        run.compact(
            jackin_telemetry::schema::events::CAPSULE_SESSION_DETACH,
            "operator detached from agent session",
        );
    }
    jackin_diagnostics::reassert_alt_screen();
    admission_lease.ensure_current(paths)?;
    eprintln!();
    result?;
    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
        &container,
    )
    .await
}

pub async fn hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
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
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    use crate::runtime::backend::ContainerBackend as _;

    match crate::runtime::backend::backend_for_state(paths, container_name) {
        crate::runtime::backend::InstanceBackend::Docker => {
            crate::runtime::backend::DockerBackend::new(docker)
                .hardline(paths, container_name, focus_session, runner)
                .await
        }
        crate::runtime::backend::InstanceBackend::AppleContainer => {
            crate::runtime::backend::AppleContainerBackend::production()
                .hardline(paths, container_name, focus_session, runner)
                .await
        }
    }
}

pub(crate) async fn hardline_docker_agent_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "hardline_container_inspect",
        Some(container_name),
    );
    let inspection = docker.inspect_container_by_name(container_name).await;
    let container_state = inspection.state;
    let container_handle = inspection.handle;
    let container_state_label = container_state.short_label();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "hardline_container_inspect",
        Some(&container_state_label),
    );
    match container_state {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            let Some(container) = container_handle else {
                anyhow::bail!("container '{container_name}' inspection returned no immutable ID");
            };
            let admission_lease = require_current_account_admission(paths, container_name)?;
            hardline_docker_agent_with_focus_with_lease(
                paths,
                container_name,
                focus_session,
                &admission_lease,
                docker,
                runner,
                &container,
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
                "OOM killed".to_owned()
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
    }
}

async fn hardline_docker_agent_with_focus_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)
        .map_err(mark_reconnect_admission_failure)?;
    // Reconcile keep_awake right before reconnect. The attach blocks on the
    // capsule exec until the session ends, so the post-hardline reconcile in
    // the app layer would fire too late.
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    let attach_outcome = reconnect_or_create_session_with_container_handle_with_lease(
        paths,
        container_name,
        focus_session,
        admission_lease,
        docker,
        runner,
        container,
    )
    .await;
    // A clean last-session shutdown surfaces as a non-zero attach result (the
    // capsule client hits the socket close as `early eof`). Preserve that one
    // known transport race, but never swallow admission or generation errors
    // while the lifecycle inspect still says the container is running.
    if let Err(error) = attach_outcome {
        if error.is::<super::launch::GenerationLeaseViolation>()
            || error.is::<ReconnectAdmissionFailure>()
        {
            return Err(error);
        }
        let inspect = docker.inspect_container_by_id(container).await;
        if let Some(diag) = super::launch::diagnose_with_state_by_id(
            runner,
            container,
            &inspect,
            super::launch::ExitPhase::PostAttach,
        )
        .await
        {
            return Err(diag);
        }
        if !super::launch::is_known_socket_close(&error, &inspect) {
            return Err(error);
        }
        let _warning = jackin_telemetry::record_recovered_degradation();
    }

    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        container,
    )
    .await
}

pub(crate) async fn finalize_reconnected_foreground_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let admission_lease = require_current_account_admission(paths, container_name)?;
    finalize_reconnected_foreground_session_with_lease(
        paths,
        container_name,
        &admission_lease,
        docker,
        runner,
    )
    .await
}

pub(crate) async fn finalize_reconnected_foreground_session_with_lease(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    let inspection = docker.inspect_container_by_name(container_name).await;
    let container = inspection
        .handle
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot resolve container {container_name}: {}",
                inspection.state.inspect_label()
            )
        })
        .map_err(mark_reconnect_admission_failure)?;
    finalize_reconnected_foreground_session_with_handle(
        paths,
        container_name,
        admission_lease,
        docker,
        runner,
        &container,
    )
    .await
}

pub(super) async fn finalize_reconnected_foreground_session_with_handle(
    paths: &JackinPaths,
    container_name: &str,
    admission_lease: &super::launch::AccountConfigRevision,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
    container: &ContainerHandle,
) -> anyhow::Result<()> {
    validate_current_account_admission(paths, container_name, admission_lease)?;
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "post_attach_outcome_inspect",
        Some(container_name),
    );
    admission_lease.ensure_current(paths)?;
    let mut outcome =
        crate::runtime::launch::inspect_attach_outcome_by_id(docker, container).await?;
    admission_lease.ensure_current(paths)?;
    let outcome_label = outcome.as_label();
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "post_attach_outcome_inspect",
        Some(&outcome_label),
    );
    super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    // The dirty-exit decision is made in-capsule (the dirty-exit modal) and
    // recorded in exit-action.json; the host only executes it — no host dialog.
    let mut prompt = crate::isolation::finalize::ExitActionPrompt {
        state_dir: paths.data_dir.join(container_name).join("state"),
    };
    jackin_diagnostics::active_timing_started(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "foreground_session_finalize",
        Some(container_name),
    );
    let mut decision = crate::isolation::finalize::finalize_foreground_session(
        crate::isolation::finalize::FinalizeContext {
            container_name,
            container_state_dir: &paths.data_dir.join(container_name),
            outcome,
            is_interactive: interactive,
            dirty_exit_policy: jackin_config::DirtyExitPolicy::Ask,
            prompt: &mut prompt,
            docker,
            runner,
            container: container.clone(),
        },
    )
    .await?;
    admission_lease.ensure_current(paths)?;
    jackin_diagnostics::active_timing_done(
        jackin_diagnostics::DiagnosticStage::Hardline,
        "foreground_session_finalize",
        Some(decision.as_str()),
    );

    if matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::ReturnToAgent
    ) {
        admission_lease.ensure_current(paths)?;
        reconnect_or_create_session_with_container_handle_with_lease(
            paths,
            container_name,
            None,
            admission_lease,
            docker,
            runner,
            container,
        )
        .await?;
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "post_attach_outcome_inspect",
            Some(container_name),
        );
        admission_lease.ensure_current(paths)?;
        outcome = crate::runtime::launch::inspect_attach_outcome_by_id(docker, container).await?;
        admission_lease.ensure_current(paths)?;
        let outcome_label = outcome.as_label();
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "post_attach_outcome_inspect",
            Some(&outcome_label),
        );
        super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
        jackin_diagnostics::active_timing_started(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "foreground_session_finalize",
            Some(container_name),
        );
        decision = crate::isolation::finalize::finalize_foreground_session(
            crate::isolation::finalize::FinalizeContext {
                container_name,
                container_state_dir: &paths.data_dir.join(container_name),
                outcome,
                is_interactive: interactive,
                dirty_exit_policy: jackin_config::DirtyExitPolicy::Ask,
                prompt: &mut prompt,
                docker,
                runner,
                container: container.clone(),
            },
        )
        .await?;
        admission_lease.ensure_current(paths)?;
        jackin_diagnostics::active_timing_done(
            jackin_diagnostics::DiagnosticStage::Hardline,
            "foreground_session_finalize",
            Some(decision.as_str()),
        );
    }

    finalize_reconnected_resources(paths, container_name, outcome, decision, docker, container)
        .await
}

async fn finalize_reconnected_resources(
    paths: &JackinPaths,
    container_name: &str,
    outcome: crate::isolation::finalize::AttachOutcome,
    decision: crate::isolation::finalize::FinalizeDecision,
    docker: &impl DockerApi,
    container: &ContainerHandle,
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
        InstanceStatus::CleanExited
    };
    if let Some(mut manifest) = InstanceManifest::read_optional_lossy(&state_dir) {
        super::launch::write_instance_status(paths, &state_dir, &mut manifest, status)?;
    }
    let dind_handle =
        super::cleanup::resolve_dind_handle_for_state(paths, container_name, docker).await?;
    super::cleanup::eject_docker_role_with_handles(
        paths,
        container_name,
        docker,
        container,
        dind_handle.as_ref(),
    )
    .await
}

pub async fn inspect_hardline_instance(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<String> {
    let state_dir = paths.data_dir.join(container_name);
    // `--inspect` is the operator's recovery tool. Distinguish "no
    // manifest yet" (pre-restore) from "manifest unreadable" (torn
    // JSON) so the render below does not lie about the latter.
    let manifest_result: Result<Option<InstanceManifest>, String> =
        InstanceManifest::read_optional(&state_dir).map_err(|e| e.to_string());
    let manifest = manifest_result.as_ref().ok().and_then(Option::as_ref);
    let resources = crate::instance::DockerResources::from_container_name(container_name);
    let dind_name = manifest.map_or_else(
        || resources.dind_container.clone(),
        |manifest| manifest.docker.dind_container.clone(),
    );
    let network_name = manifest.as_ref().map_or_else(
        || resources.network.clone(),
        |manifest| manifest.docker.network.clone(),
    );
    let certs_volume = manifest.as_ref().map_or_else(
        || resources.certs_volume.clone(),
        |manifest| manifest.docker.certs_volume.clone(),
    );

    let (role_inspection, dind_state_raw, network_result) = tokio::join!(
        docker.inspect_container_by_name(container_name),
        async {
            if let Some(dind_name) = dind_name.as_deref() {
                let inspection = docker.inspect_container_by_name(dind_name).await;
                Some(match inspection.handle {
                    Some(handle) => docker.inspect_container_by_id(&handle).await,
                    None => inspection.state,
                })
            } else {
                None
            }
        },
        inspect_docker_network(docker, &network_name),
    );
    let role_container_state = role_inspection.state;
    let sessions = match role_inspection.handle {
        Some(container) => inspect_agent_sessions(docker, &container, &role_container_state).await,
        None => AgentSessionInventory::NotRunning,
    };
    let role_state = role_container_state.inspect_label();
    let dind_state = dind_state_raw
        .as_ref()
        .map_or_else(|| "disabled".to_owned(), ContainerState::inspect_label);
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
            for admitted in &manifest.admitted_instances {
                if admitted.registration_state != RegistrationState::Current {
                    lines.push(format!(
                        "Registration {} ({}): {}",
                        admitted.config_id,
                        admitted.account_id,
                        admitted.registration_state.label()
                    ));
                }
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
        Ok(None) => lines.push("Manifest: missing".to_owned()),
        Err(error) => lines.push(format!("Manifest: unreadable ({error})")),
    }

    lines.extend([
        format!("Role container: {container_name} ({role_state})"),
        format!("Agent sessions: {}", describe_agent_sessions(&sessions)),
        format!(
            "DinD container: {} ({dind_state})",
            dind_name.as_deref().unwrap_or("none")
        ),
        format!("Docker network: {network_name} ({network_state})"),
        format!(
            "DinD cert volume: {}",
            certs_volume.as_deref().unwrap_or("none")
        ),
        format!("Mounts: {mounts}"),
    ]);
    Ok(lines.join("\n"))
}

pub fn describe_agent_session_count(sessions: &AgentSessionInventory) -> String {
    match sessions {
        AgentSessionInventory::NotRunning => "sessions:not_running".to_owned(),
        AgentSessionInventory::Unavailable(_) => "sessions:unavailable".to_owned(),
        AgentSessionInventory::Sessions(sessions) => format!("sessions:{}", sessions.len()),
    }
}

fn describe_agent_sessions(sessions: &AgentSessionInventory) -> String {
    match sessions {
        AgentSessionInventory::NotRunning => "not running".to_owned(),
        AgentSessionInventory::Unavailable(reason) => format!("unavailable: {reason}"),
        AgentSessionInventory::Sessions(sessions) if sessions.is_empty() => {
            "none detected".to_owned()
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
        DockerNetworkState::Present => "present".to_owned(),
        DockerNetworkState::NotFound => "missing".to_owned(),
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

async fn inspect_docker_network(docker: &impl DockerApi, network: &str) -> DockerNetworkState {
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
    dind: &ContainerHandle,
    certs_volume: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    const MAX_ATTEMPTS: u32 = 30;
    const INITIAL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);
    const MAX_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

    // Shared spinner helper: it suppresses its own stderr output while the
    // rich launch cockpit owns the screen, so the sidecar stage shows only
    // in the rail rather than streaming "Waiting for ..." over the frame.
    crate::spin_wait::spin_wait_ramped(
        "Waiting for Docker-in-Docker to be ready",
        MAX_ATTEMPTS,
        INITIAL_INTERVAL,
        MAX_INTERVAL,
        || async {
            docker
                .exec_capture_by_id(dind, &["docker", "info"])
                .await
                .map(|_| ())
        },
    )
    .await
    .with_context(|| {
        format!(
            "timed out waiting for Docker-in-Docker sidecar {}",
            dind.name()
        )
    })?;

    match docker
        .exec_capture_by_id(dind, &["test", "-f", "/certs/client/ca.pem"])
        .await
    {
        Ok(_) => {}
        Err(e) if e.to_string().contains("exited with code") => {
            anyhow::bail!(
                "DinD TLS client certificates not found on volume {certs_volume} — \
                 the DinD sidecar may have started without generating certificates"
            );
        }
        Err(e) => return Err(e.context(format!("checking TLS cert presence in {}", dind.name()))),
    }

    Ok(())
}

#[cfg(test)]
mod tests;
