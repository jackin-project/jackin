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

use crate::instance::{InstanceManifest, InstanceStatus};
use anyhow::Context as _;
use jackin_core::container_paths;
use jackin_core::{CommandRunner, JACKIN_STATUS_CMD, RunOptions};
use jackin_docker::docker_client::DockerApi;
use jackin_protocol::attach::SpawnRequest;
use std::path::PathBuf;

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

pub fn attach_proxy_exec_args(container_name: &str) -> Vec<String> {
    vec![
        "exec".to_owned(),
        "-i".to_owned(),
        container_name.to_owned(),
        JACKIN_CAPSULE_PATH.to_owned(),
        ATTACH_PROXY_SUBCOMMAND.to_owned(),
    ]
}

/// Conservative `sockaddr_un.sun_path` capacity across the platforms jackin'
/// targets (macOS/BSD = 104, Linux = 108). A socket path at or above this cannot
/// be `connect`ed directly — the kernel rejects it — so the direct transport is
/// impossible regardless of whether the socket exists.
const MAX_UNIX_SOCKET_PATH_LEN: usize = 104;

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
        tracing::warn!(
            otel.name = "attach:socket_path_over_sun_len",
            path_len,
            "{reason}"
        );
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

    match std::os::unix::net::UnixStream::connect(&socket_path) {
        Ok(_) => HostAttachTransportPlan::DirectSocket { socket_path },
        Err(err) => HostAttachTransportPlan::AttachProxy {
            socket_path,
            direct_error: Some(err.to_string()),
        },
    }
}

pub(super) async fn wait_for_capsule_daemon(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
) -> anyhow::Result<()> {
    const MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(30);
    const INITIAL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);
    const MAX_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

    jackin_diagnostics::active_timing_started(
        "capsule",
        "wait_capsule_socket",
        Some(container_name),
    );
    let wait_result = wait_for_capsule_daemon_ready(
        paths,
        container_name,
        docker,
        MAX_WAIT,
        INITIAL_INTERVAL,
        MAX_INTERVAL,
    )
    .await
    .with_context(|| format!("waiting for jackin-capsule daemon in {container_name}"));
    jackin_diagnostics::active_timing_done(
        "capsule",
        "wait_capsule_socket",
        if wait_result.is_ok() {
            Some("ready")
        } else {
            Some("error")
        },
    );
    wait_result
}

async fn wait_for_capsule_daemon_ready(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    max_wait: std::time::Duration,
    initial_interval: std::time::Duration,
    max_interval: std::time::Duration,
) -> anyhow::Result<()> {
    let started = tokio::time::Instant::now();
    let mut interval = initial_interval;

    loop {
        if capsule_daemon_socket_connects(paths, container_name) {
            return Ok(());
        }

        let Err(exec_error) = docker
            .exec_capture(container_name, &["sh", "-c", JACKIN_STATUS_CMD])
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
    socket_path.exists() && std::os::unix::net::UnixStream::connect(socket_path).is_ok()
}

#[cfg(test)]
use crate::instance::InstanceIndex;
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
    container_name: &str,
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
    let title = match InstanceManifest::read(&paths.data_dir.join(container_name)) {
        Ok(m) => m.role_display_name,
        Err(e) => {
            jackin_diagnostics::debug_log!(
                "attach",
                "set_role_terminal_title: manifest read failed for {container_name}: {e:#}; \
                 using container name as title",
            );
            container_name.to_owned()
        }
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

/// Insert `--user <host-uid>:<host-gid>` right after `exec` so a `docker exec`
/// shell runs as the same host identity the container was launched with
/// (`--user` on `docker run`). Without it the exec would default to the image's
/// baked `agent` user (UID 1000) and hit the same bind-mount ownership mismatch
/// the run-time identity mapping exists to remove. No-op on non-unix hosts.
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

pub(super) async fn reconnect_or_create_session_with_focus(
    paths: &JackinPaths,
    container_name: &str,
    focus_session: Option<u64>,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    set_role_terminal_title(paths, container_name);
    wait_for_capsule_daemon(paths, container_name, docker).await?;
    if super::host_attach::host_attach_enabled() {
        let outcome = super::host_attach::run_host_attach_session(
            paths,
            container_name,
            None,
            focus_session,
            &[],
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        return outcome;
    }
    let focus_arg = focus_session.map(|id| id.to_string());
    let run_as_user = crate::runtime::identity::host_run_as_user();
    let mut args: Vec<&str> = vec!["exec", "-it", container_name, container_paths::CAPSULE_BIN];
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    insert_run_as_user(&mut args, run_as_user.as_deref());
    if let Some(ref id) = focus_arg {
        args.push("--focus");
        args.push(id);
    }
    jackin_diagnostics::active_timing_started(
        "hardline",
        "capsule_client_exec",
        Some(container_name),
    );
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
        "hardline",
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
            jackin_diagnostics::otel_events::SESSION_DETACH,
            "operator detached from capsule session",
        );
    }
    // The capsule has detached; re-claim the alt screen before any post-attach
    // work so the exit flow does not flash the operator's shell.
    jackin_diagnostics::reassert_alt_screen();
    outcome
}

pub(super) async fn start_or_reconnect_capsule_client(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    jackin_diagnostics::active_timing_started("capsule", "restore_inspect", Some(container_name));
    let inspect = docker.inspect_container_state(container_name).await;
    let inspect_label = inspect.short_label();
    jackin_diagnostics::active_timing_done("capsule", "restore_inspect", Some(&inspect_label));
    match inspect {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {}
        ContainerState::Stopped { .. } | ContainerState::Created => {
            jackin_diagnostics::active_timing_started(
                "capsule",
                "restore_start_container",
                Some(container_name),
            );
            let start_result = docker
                .start_container(container_name)
                .await
                .with_context(|| format!("starting role container {container_name}"));
            jackin_diagnostics::active_timing_done(
                "capsule",
                "restore_start_container",
                if start_result.is_ok() {
                    Some("started")
                } else {
                    Some("error")
                },
            );
            start_result?;
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
    reconnect_or_create_session_with_focus(paths, container_name, None, docker, runner).await
}

pub(super) async fn start_or_hardline_agent(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    use crate::runtime::backend::ContainerBackend as _;

    match crate::runtime::backend::backend_for_state(paths, container_name) {
        crate::runtime::backend::InstanceBackend::Docker => {
            let backend = crate::runtime::backend::DockerBackend::new(docker);
            backend
                .reconnect(paths, container_name, None, runner)
                .await?;
            backend.finalize(paths, container_name, runner).await
        }
        crate::runtime::backend::InstanceBackend::AppleContainer => {
            let backend = crate::runtime::backend::AppleContainerBackend::production();
            backend
                .reconnect(paths, container_name, None, runner)
                .await?;
            backend.finalize(paths, container_name, runner).await
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
    docker: &impl DockerApi,
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
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    if super::host_attach::host_attach_enabled() {
        let result = super::host_attach::run_host_attach_session(
            paths,
            container_name,
            Some(SpawnRequest::Shell),
            None,
            &[],
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        eprintln!();
        result?;
        return finalize_reconnected_foreground_session(paths, container_name, docker, runner)
            .await;
    }
    let run_as_user = crate::runtime::identity::host_run_as_user();
    let mut args: Vec<&str> = vec![
        "exec",
        "-it",
        container_name,
        container_paths::CAPSULE_BIN,
        "new",
    ];
    insert_run_as_user(&mut args, run_as_user.as_deref());
    if let Some(flag) = host_alt_screen_exec_flag() {
        args.insert(1, flag);
    }
    jackin_diagnostics::active_timing_started(
        "hardline",
        "shell_session_exec",
        Some(container_name),
    );
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
        "hardline",
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
            jackin_diagnostics::otel_events::SESSION_DETACH,
            "operator detached from shell session",
        );
    }
    jackin_diagnostics::reassert_alt_screen();
    eprintln!();
    result?;
    finalize_reconnected_foreground_session(paths, container_name, docker, runner).await
}

#[expect(
    clippy::too_many_arguments,
    reason = "Spawning a single agent session requires every caller-supplied \
              parameter (paths, container_name, manifest, agent, provider_label, \
              env_overrides, git config, docker, runner, ...) to flow through to \
              the container bring-up path; bundling into a config struct would be \
              a parallel pass that requires restructuring the spawn path. Named- \
              arg reads match the per-input propagation idiom."
)]
pub async fn spawn_agent_session(
    paths: &JackinPaths,
    container_name: &str,
    manifest: Option<&InstanceManifest>,
    agent: jackin_core::Agent,
    provider_label: Option<&str>,
    env_overrides: &[(String, String)],
    git_coauthor_trailer: bool,
    git_dco: bool,
    docker: &impl DockerApi,
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

    // Agent selection travels as `jackin-capsule new <agent>` argv; the
    // git policy toggles are session env consumed by the spawned entrypoint.
    // Each transport encodes them only on the path that consumes it.
    set_role_terminal_title(paths, container_name);
    jackin_host::caffeinate::reconcile(paths, docker, runner).await;
    if super::host_attach::host_attach_enabled() {
        let mut session_env_overrides: Vec<(String, String)> =
            git_policy_env_pairs(git_coauthor_trailer, git_dco)
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect();
        session_env_overrides.extend(env_overrides.iter().cloned());
        let spawn_request = if let Some(provider_label) = provider_label {
            SpawnRequest::AgentWithProvider {
                slug: agent.slug().to_owned(),
                provider_label: provider_label.to_owned(),
            }
        } else {
            SpawnRequest::agent(agent.slug())?
        };
        let result = super::host_attach::run_host_attach_session(
            paths,
            container_name,
            Some(spawn_request),
            None,
            &session_env_overrides,
        )
        .await;
        jackin_diagnostics::reassert_alt_screen();
        eprintln!();
        result?;
        return finalize_reconnected_foreground_session(paths, container_name, docker, runner)
            .await;
    }

    let run_as_user = crate::runtime::identity::host_run_as_user();
    let mut exec_args = vec!["exec", "--workdir", workdir, "-it"];
    insert_run_as_user(&mut exec_args, run_as_user.as_deref());
    // git policy toggles then provider env overrides (e.g. ANTHROPIC_AUTH_TOKEN +
    // ANTHROPIC_BASE_URL for Z.AI) as docker `-e` flags. Owned so they outlive
    // `exec_args`.
    let env_flags: Vec<String> = git_policy_env_pairs(git_coauthor_trailer, git_dco)
        .into_iter()
        .map(|(name, value)| format!("-e={name}={value}"))
        .chain(env_overrides.iter().map(|(k, v)| format!("-e={k}={v}")))
        .collect();
    for flag in &env_flags {
        exec_args.push(flag.as_str());
    }
    exec_args.push(container_name);
    exec_args.extend_from_slice(&[container_paths::CAPSULE_BIN, "new", agent.slug()]);
    // When a provider was selected in the console, pass it as a flag so the
    // daemon receives SpawnRequest::AgentWithProvider and labels the tab correctly.
    let provider_flag = provider_label.map(|label| format!("--provider={label}"));
    if let Some(ref flag) = provider_flag {
        exec_args.push(flag.as_str());
    }
    if let Some(flag) = host_alt_screen_exec_flag() {
        exec_args.insert(1, flag);
    }
    let timing_name = format!("new_{}_session_exec", agent.slug());
    jackin_diagnostics::active_timing_started("hardline", &timing_name, Some(container_name));
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
        "hardline",
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
            jackin_diagnostics::otel_events::SESSION_DETACH,
            "operator detached from agent session",
        );
    }
    jackin_diagnostics::reassert_alt_screen();
    eprintln!();
    result?;
    finalize_reconnected_foreground_session(paths, container_name, docker, runner).await
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
    // Reconcile keep_awake right before each `reconnect_or_create_session_with_focus`
    // call. The attach blocks on the jackin-capsule exec until the session ends,
    // so the post-hardline reconcile in `app::Command::Hardline` would fire
    // too late. Firing here, while the container is observably running, ensures
    // caffeinate spawns for the duration of the re-attached session.
    jackin_diagnostics::active_timing_started(
        "hardline",
        "hardline_container_inspect",
        Some(container_name),
    );
    let container_state = docker.inspect_container_state(container_name).await;
    let container_state_label = container_state.short_label();
    jackin_diagnostics::active_timing_done(
        "hardline",
        "hardline_container_inspect",
        Some(&container_state_label),
    );
    let attach_outcome = match container_state {
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting => {
            jackin_host::caffeinate::reconcile(paths, docker, runner).await;
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
    };
    // A clean last-session shutdown surfaces as a non-zero attach result (the
    // capsule client hits the socket close as `early eof`). Do not short-circuit
    // on it: `finalize_reconnected_foreground_session` re-inspects the container
    // and reads exit-action.json, so it handles both a clean exit and a genuine
    // failure. Only a clean exit reaches here in practice; log and proceed.
    if let Err(err) = attach_outcome {
        jackin_diagnostics::debug_log!(
            "hardline",
            "attach for {container_name} ended with ({err}); proceeding to finalize"
        );
    }

    finalize_reconnected_foreground_session(paths, container_name, docker, runner).await
}

pub(crate) async fn finalize_reconnected_foreground_session(
    paths: &JackinPaths,
    container_name: &str,
    docker: &impl DockerApi,
    runner: &mut impl CommandRunner,
) -> anyhow::Result<()> {
    jackin_diagnostics::active_timing_started(
        "hardline",
        "post_attach_outcome_inspect",
        Some(container_name),
    );
    let mut outcome =
        crate::runtime::launch::inspect_attach_outcome(docker, container_name).await?;
    let outcome_label = outcome.as_label();
    jackin_diagnostics::active_timing_done(
        "hardline",
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
        "hardline",
        "foreground_session_finalize",
        Some(container_name),
    );
    let mut decision = crate::isolation::finalize::finalize_foreground_session(
        container_name,
        &paths.data_dir.join(container_name),
        outcome,
        interactive,
        jackin_config::DirtyExitPolicy::Ask,
        &mut prompt,
        docker,
        runner,
    )
    .await?;
    jackin_diagnostics::active_timing_done(
        "hardline",
        "foreground_session_finalize",
        Some(decision.as_str()),
    );

    if matches!(
        decision,
        crate::isolation::finalize::FinalizeDecision::ReturnToAgent
    ) {
        start_or_reconnect_capsule_client(paths, container_name, docker, runner).await?;
        jackin_diagnostics::active_timing_started(
            "hardline",
            "post_attach_outcome_inspect",
            Some(container_name),
        );
        outcome = crate::runtime::launch::inspect_attach_outcome(docker, container_name).await?;
        let outcome_label = outcome.as_label();
        jackin_diagnostics::active_timing_done(
            "hardline",
            "post_attach_outcome_inspect",
            Some(&outcome_label),
        );
        super::launch::record_instance_attach_outcome(paths, container_name, outcome)?;
        jackin_diagnostics::active_timing_started(
            "hardline",
            "foreground_session_finalize",
            Some(container_name),
        );
        decision = crate::isolation::finalize::finalize_foreground_session(
            container_name,
            &paths.data_dir.join(container_name),
            outcome,
            interactive,
            jackin_config::DirtyExitPolicy::Ask,
            &mut prompt,
            docker,
            runner,
        )
        .await?;
        jackin_diagnostics::active_timing_done(
            "hardline",
            "foreground_session_finalize",
            Some(decision.as_str()),
        );
    }

    finalize_reconnected_resources(paths, container_name, outcome, decision, docker).await
}

async fn finalize_reconnected_resources(
    paths: &JackinPaths,
    container_name: &str,
    outcome: crate::isolation::finalize::AttachOutcome,
    decision: crate::isolation::finalize::FinalizeDecision,
    docker: &impl DockerApi,
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
    if let Some(mut manifest) =
        InstanceManifest::read_or_log(&state_dir, "finalize_reconnected_resources")
    {
        super::launch::write_instance_status(paths, &state_dir, &mut manifest, status)?;
    }
    super::cleanup::eject_role(paths, container_name, docker).await
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

    let (role_container_state, dind_state_raw, network_result) = tokio::join!(
        docker.inspect_container_state(container_name),
        async {
            if let Some(dind_name) = dind_name.as_deref() {
                Some(docker.inspect_container_state(dind_name).await)
            } else {
                None
            }
        },
        inspect_docker_network(docker, &network_name),
    );
    let sessions = inspect_agent_sessions(docker, container_name, &role_container_state).await;
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
    dind_name: &str,
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
mod tests;
