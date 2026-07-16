//! Minimal host-daemon lifecycle foundation.
//!
//! This module intentionally owns only the empty daemon shell: socket binding,
//! request/response framing, status, and shutdown. Reactive adapters are added
//! by later plans.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use jackin_core::{ContainerId, JackinPaths};
use jackin_protocol::control::AgentState;
use jackin_protocol::{InstanceSnapshot, TelemetryContext};
use serde::{Deserialize, Serialize};

pub const DAEMON_PROTOCOL_VERSION: u16 = 2;
pub const MAX_REQUEST_BYTES: u64 = 16 * 1024;
pub const SOCKET_FILE_NAME: &str = "jackin-daemon.sock";
pub const PID_FILE_NAME: &str = "jackin-daemon.pid";
const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonRequest {
    pub id: String,
    pub protocol_version: u16,
    pub build_id: String,
    pub ctx: TelemetryContext,
    #[serde(flatten)]
    pub kind: DaemonRequestKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonRequestKind {
    Hello,
    Status,
    TelemetryHealth,
    AttentionSnapshot {
        container_name: String,
        panes: Vec<AttentionPaneStatus>,
    },
    Shutdown,
}

impl DaemonRequestKind {
    const fn rpc_method(&self) -> &'static str {
        match self {
            Self::Hello => "jackin.host.Daemon/Hello",
            Self::Status => "jackin.host.Daemon/Status",
            Self::TelemetryHealth => "jackin.host.Daemon/TelemetryHealth",
            Self::AttentionSnapshot { .. } => "jackin.host.Daemon/AttentionSnapshot",
            Self::Shutdown => "jackin.host.Daemon/Shutdown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonResponse {
    pub id: String,
    #[serde(flatten)]
    pub kind: DaemonResponseKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonResponseKind {
    Hello {
        protocol_version: u16,
        build_id: String,
        capabilities: Vec<String>,
    },
    Status(DaemonStatus),
    TelemetryHealth(TelemetryHealthReport),
    AttentionAccepted {
        notifications: usize,
        muted: bool,
    },
    Shutdown {
        accepted: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonStatus {
    pub protocol_version: u16,
    pub build_id: String,
    pub pid: u32,
    pub socket_path: PathBuf,
    pub coredump_policy: CoredumpPolicy,
    pub adapters_enabled: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryHealthReport {
    pub fingerprint: SanitizedConfigFingerprint,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_failure: Option<TelemetryConfigFailure>,
    pub health: TelemetryHealthSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SanitizedConfigFingerprint {
    pub traces: Option<TelemetrySignalConfigFingerprint>,
    pub logs: Option<TelemetrySignalConfigFingerprint>,
    pub metrics: Option<TelemetrySignalConfigFingerprint>,
    pub compression: String,
    pub sampler: String,
    pub active_signals: u8,
    pub service_name: String,
    pub app_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetrySignalConfigFingerprint {
    pub authority: String,
    pub tls: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryConfigFailure {
    MissingSignalEndpoint,
    UnsupportedProtocol,
    ConflictingSampler,
    UnsupportedCompression,
    InvalidTimeout,
    InvalidHeaders,
    InvalidResourceAttributes,
    InvalidEndpoint,
    EmptyValue,
    IncompleteClientIdentity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryHealthSnapshot {
    pub active_signals: u8,
    pub traces: TelemetrySignalHealth,
    pub logs: TelemetrySignalHealth,
    pub metrics: TelemetrySignalHealth,
    pub facade_rejections: u64,
    pub flush: TelemetryFlushStatus,
    pub shutdown_completed: bool,
    pub shutdown_succeeded: bool,
    pub shutdown_timed_out: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TelemetryFlushStatus {
    Pending,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetrySignalHealth {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttentionPaneStatus {
    pub session_id: u64,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub state: AgentState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttentionNotification {
    pub container_name: String,
    pub session_id: u64,
    pub agent: Option<String>,
    pub label: String,
    pub state: AgentState,
}

pub trait AttentionNotifier {
    fn notify(&mut self, notification: &AttentionNotification) -> Result<()>;
    fn muted(&self) -> bool;
}

pub trait NotificationDispatcher {
    fn dispatch(&mut self, command: &NotificationCommand) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Default)]
pub struct StdNotificationDispatcher;

impl NotificationDispatcher for StdNotificationDispatcher {
    fn dispatch(&mut self, command: &NotificationCommand) -> Result<()> {
        let request = jackin_process::ExecRequest::new(&command.program, &command.args)
            .stdin_mode(jackin_process::StdioMode::Inherit)
            .stdout_mode(jackin_process::StdioMode::Inherit)
            .stderr_mode(jackin_process::StdioMode::Inherit);
        let status = jackin_process::exec_sync(&request)
            .with_context(|| format!("dispatching notification via {}", command.program))?;
        if status.success {
            Ok(())
        } else {
            bail!(
                "notification command {} exited with code {:?}",
                command.program,
                status.code
            )
        }
    }
}

#[derive(Debug)]
pub struct HostAttentionNotifier<D> {
    dispatcher: D,
    enabled: bool,
}

impl<D> HostAttentionNotifier<D> {
    pub const fn new(dispatcher: D, enabled: bool) -> Self {
        Self {
            dispatcher,
            enabled,
        }
    }
}

impl<D: NotificationDispatcher> AttentionNotifier for HostAttentionNotifier<D> {
    fn notify(&mut self, notification: &AttentionNotification) -> Result<()> {
        let title = format!(
            "{} needs attention",
            notification.agent.as_deref().unwrap_or("agent")
        );
        let body = format!(
            "{} in {} is {}",
            notification.label,
            notification.container_name,
            notification.state.label()
        );
        let title = jackin_diagnostics::scrub_secrets(&title).into_owned();
        let body = jackin_diagnostics::scrub_secrets(&body).into_owned();
        jackin_diagnostics::telemetry_debug!(
            "daemon",
            "attention state={} container={} session={} muted={}",
            notification.state.label(),
            notification.container_name,
            notification.session_id,
            !self.enabled
        );
        if !self.enabled {
            return Ok(());
        }
        match notification_command_for_host(&title, &body) {
            Some(command) => self.dispatcher.dispatch(&command),
            None => Ok(()),
        }
    }

    fn muted(&self) -> bool {
        !self.enabled
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SessionKey {
    container_name: ContainerId,
    session_id: u64,
}

#[derive(Debug)]
pub struct AttentionAdapter<N> {
    notifier: N,
    last_seen: HashMap<SessionKey, AgentState>,
}

impl<N: AttentionNotifier> AttentionAdapter<N> {
    pub fn new(notifier: N) -> Self {
        Self {
            notifier,
            last_seen: HashMap::new(),
        }
    }

    pub fn ingest_snapshot(
        &mut self,
        container_name: &str,
        snapshot: &InstanceSnapshot,
    ) -> Result<usize> {
        let panes = snapshot
            .tabs
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .map(|pane| AttentionPaneStatus {
                session_id: pane.session_id,
                label: pane.label.clone(),
                agent: pane.agent.clone(),
                state: pane.state,
            })
            .collect::<Vec<_>>();
        self.ingest_panes(container_name, &panes)
    }

    pub fn ingest_panes(
        &mut self,
        container_name: &str,
        panes: &[AttentionPaneStatus],
    ) -> Result<usize> {
        let container_id = ContainerId::parse(container_name)
            .context("validating attention snapshot container name")?;
        let mut sent = 0;
        for pane in panes {
            let key = SessionKey {
                container_name: container_id.clone(),
                session_id: pane.session_id,
            };
            let previous = self.last_seen.insert(key, pane.state);
            if previous == Some(pane.state) || !is_attention_state(pane.state) {
                continue;
            }
            self.notifier.notify(&AttentionNotification {
                container_name: container_name.to_owned(),
                session_id: pane.session_id,
                agent: pane.agent.clone(),
                label: pane.label.clone(),
                state: pane.state,
            })?;
            sent += usize::from(!self.notifier.muted());
        }
        Ok(sent)
    }

    pub fn muted(&self) -> bool {
        self.notifier.muted()
    }

    pub fn into_notifier(self) -> N {
        self.notifier
    }
}

const fn is_attention_state(state: AgentState) -> bool {
    matches!(state, AgentState::Blocked | AgentState::Done)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CoredumpPolicy {
    Disabled,
    Unsupported { residual_risk: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeOutcome {
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonLayout {
    pub run_dir: PathBuf,
    pub socket_path: PathBuf,
    pub pid_path: PathBuf,
}

impl DaemonLayout {
    #[must_use]
    pub fn new(paths: &JackinPaths) -> Self {
        let run_dir = paths.jackin_home.join("run");
        Self {
            socket_path: run_dir.join(SOCKET_FILE_NAME),
            pid_path: run_dir.join(PID_FILE_NAME),
            run_dir,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitFiles {
    pub launchd_label: String,
    pub launchd_path: PathBuf,
    pub launchd_plist: String,
    pub systemd_path: PathBuf,
    pub systemd_unit: String,
}

pub fn ensure_run_dir(layout: &DaemonLayout) -> Result<()> {
    fs::create_dir_all(&layout.run_dir)
        .with_context(|| format!("creating daemon run dir {}", layout.run_dir.display()))?;
    fs::set_permissions(&layout.run_dir, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("restricting daemon run dir {}", layout.run_dir.display()))
}

pub fn bind_control_socket(layout: &DaemonLayout) -> Result<UnixListener> {
    ensure_run_dir(layout)?;
    if layout.socket_path.exists() {
        fs::remove_file(&layout.socket_path)
            .with_context(|| format!("removing stale socket {}", layout.socket_path.display()))?;
    }
    let listener = UnixListener::bind(&layout.socket_path)
        .with_context(|| format!("binding daemon socket {}", layout.socket_path.display()))?;
    fs::set_permissions(&layout.socket_path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restricting daemon socket {}", layout.socket_path.display()))?;
    Ok(listener)
}

pub fn serve(layout: &DaemonLayout, build_id: &str) -> Result<ServeOutcome> {
    let listener = bind_control_socket(layout)?;
    write_pid(layout)?;
    let coredump_policy = disable_coredumps();
    let attention_enabled = std::env::var_os("JACKIN_ATTENTION").is_some_and(|value| value == "1");
    let mut attention = AttentionAdapter::new(HostAttentionNotifier::new(
        StdNotificationDispatcher,
        attention_enabled,
    ));
    for stream in listener.incoming() {
        let mut stream = stream.context("accepting daemon client")?;
        let response = handle_stream(
            &mut stream,
            layout,
            build_id,
            &coredump_policy,
            &mut attention,
        )?;
        if matches!(
            response.kind,
            DaemonResponseKind::Shutdown { accepted: true }
        ) {
            cleanup_runtime_files(layout);
            return Ok(ServeOutcome::Shutdown);
        }
    }
    Ok(ServeOutcome::Shutdown)
}

pub fn request(
    socket_path: &Path,
    build_id: &str,
    kind: DaemonRequestKind,
) -> Result<DaemonResponse> {
    let method = kind.rpc_method();
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(method),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let perform_request = || {
        let mut stream = jackin_diagnostics::operation::connection_attempt_sync(
            jackin_telemetry::schema::enums::ConnectionPeerType::HostDaemon,
            || UnixStream::connect(socket_path),
        )
        .with_context(|| format!("connecting to daemon socket {}", socket_path.display()))?;
        let mut ctx = TelemetryContext::v1();
        jackin_telemetry::propagation::inject(&mut ctx);
        let request = DaemonRequest {
            id: "cli".to_owned(),
            protocol_version: DAEMON_PROTOCOL_VERSION,
            build_id: build_id.to_owned(),
            ctx,
            kind,
        };
        serde_json::to_writer(&mut stream, &request).context("writing daemon request")?;
        stream
            .write_all(b"\n")
            .context("terminating daemon request")?;
        read_response(stream)
    };
    let response = if let Some(operation) = operation.as_ref() {
        operation.span().in_scope(perform_request)
    } else {
        perform_request()
    }
    .and_then(|response| {
        anyhow::ensure!(
            matches!(response.kind, DaemonResponseKind::Error { .. })
                || daemon_response_matches_method(method, &response.kind),
            "daemon replied with a mismatched response for {method}"
        );
        Ok(response)
    });
    if let Some(operation) = operation {
        let failed = response.as_ref().map_or(true, |response| {
            matches!(response.kind, DaemonResponseKind::Error { .. })
        });
        operation.complete(
            if failed {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            },
            failed.then_some(RPC_ERROR),
        );
    }
    response
}

fn daemon_response_matches_method(method: &str, response: &DaemonResponseKind) -> bool {
    matches!(
        (method, response),
        ("jackin.host.Daemon/Hello", DaemonResponseKind::Hello { .. })
            | ("jackin.host.Daemon/Status", DaemonResponseKind::Status(_))
            | (
                "jackin.host.Daemon/TelemetryHealth",
                DaemonResponseKind::TelemetryHealth(_)
            )
            | (
                "jackin.host.Daemon/AttentionSnapshot",
                DaemonResponseKind::AttentionAccepted { .. }
            )
            | (
                "jackin.host.Daemon/Shutdown",
                DaemonResponseKind::Shutdown { .. }
            )
    )
}

pub fn render_unit_files(paths: &JackinPaths, executable: &Path) -> UnitFiles {
    let launchd_label = "com.jackin.daemon".to_owned();
    let launchd_path = paths
        .home_dir
        .join("Library/LaunchAgents")
        .join(format!("{launchd_label}.plist"));
    let systemd_path = paths
        .config_dir
        .join("systemd/user")
        .join("jackin-daemon.service");
    let exe = executable.display();
    let launchd_plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{launchd_label}</string>
  <key>ProgramArguments</key>
  <array><string>{exe}</string><string>daemon</string><string>serve</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#
    );
    let systemd_unit = format!(
        "[Unit]\nDescription=jackin daemon\n\n[Service]\nExecStart={exe} daemon serve\nRestart=on-failure\nStandardOutput=null\nStandardError=null\n\n[Install]\nWantedBy=default.target\n"
    );
    UnitFiles {
        launchd_label,
        launchd_path,
        launchd_plist,
        systemd_path,
        systemd_unit,
    }
}

pub fn install_units(paths: &JackinPaths, executable: &Path) -> Result<UnitFiles> {
    let units = render_unit_files(paths, executable);
    if cfg!(target_os = "macos") {
        write_parented(&units.launchd_path, &units.launchd_plist)?;
    } else {
        write_parented(&units.systemd_path, &units.systemd_unit)?;
    }
    Ok(units)
}

pub fn uninstall_units(paths: &JackinPaths, executable: &Path) -> Result<UnitFiles> {
    let units = render_unit_files(paths, executable);
    remove_if_exists(&units.launchd_path)?;
    remove_if_exists(&units.systemd_path)?;
    Ok(units)
}

fn handle_stream(
    stream: &mut UnixStream,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
    attention: &mut impl AttentionNotifierAdapter,
) -> Result<DaemonResponse> {
    let mut line = String::new();
    let read = BufReader::new(stream.try_clone().context("cloning daemon stream")?)
        .take(MAX_REQUEST_BYTES + 1)
        .read_line(&mut line)
        .context("reading daemon request")?;
    let handled = if read as u64 > MAX_REQUEST_BYTES {
        HandledResponse::without_operation(error_response(
            "unknown",
            "daemon request exceeds 16384 byte limit",
        ))
    } else {
        handle_request_line_inner(
            line.trim_end(),
            layout,
            build_id,
            coredump_policy,
            attention,
        )
    };
    let response = handled.response;
    let write_result = (|| {
        serde_json::to_writer(&mut *stream, &response).context("writing daemon response")?;
        stream
            .write_all(b"\n")
            .context("terminating daemon response")
    })();
    if let Some(operation) = handled.operation {
        let failed =
            matches!(response.kind, DaemonResponseKind::Error { .. }) || write_result.is_err();
        operation.complete(
            if failed {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            },
            failed.then_some(RPC_ERROR),
        );
    }
    write_result?;
    Ok(response)
}

struct HandledResponse {
    response: DaemonResponse,
    operation: Option<jackin_telemetry::operation::OperationGuard>,
}

impl HandledResponse {
    fn without_operation(response: DaemonResponse) -> Self {
        Self {
            response,
            operation: None,
        }
    }

    fn complete_without_transport(self) -> DaemonResponse {
        let failed = matches!(self.response.kind, DaemonResponseKind::Error { .. });
        if let Some(operation) = self.operation {
            operation.complete(
                if failed {
                    jackin_telemetry::schema::enums::OutcomeValue::Failure
                } else {
                    jackin_telemetry::schema::enums::OutcomeValue::Success
                },
                failed.then_some(RPC_ERROR),
            );
        }
        self.response
    }
}

pub fn handle_request_line(
    line: &str,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
    attention: &mut impl AttentionNotifierAdapter,
) -> DaemonResponse {
    handle_request_line_inner(line, layout, build_id, coredump_policy, attention)
        .complete_without_transport()
}

fn handle_request_line_inner(
    line: &str,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
    attention: &mut impl AttentionNotifierAdapter,
) -> HandledResponse {
    if line.is_empty() {
        return HandledResponse::without_operation(error_response(
            "unknown",
            "empty daemon request",
        ));
    }
    match serde_json::from_str::<DaemonRequest>(line) {
        Ok(request) => handle_request(request, layout, build_id, coredump_policy, attention),
        Err(error) => HandledResponse::without_operation(error_response(
            "unknown",
            format!("invalid daemon request: {error}"),
        )),
    }
}

fn handle_request(
    request: DaemonRequest,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
    attention: &mut impl AttentionNotifierAdapter,
) -> HandledResponse {
    let extracted = jackin_telemetry::propagation::extract(&request.ctx);
    if matches!(
        extracted,
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        return HandledResponse::without_operation(error_response(
            request.id,
            "invalid correlation",
        ));
    }
    let method = request.kind.rpc_method();
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(method),
        },
    ];
    let operation = match &extracted {
        jackin_telemetry::propagation::ExtractOutcome::Parent(parent) => {
            jackin_telemetry::operation_with_remote_parent(
                &jackin_telemetry::operation::RPC_SERVER,
                &attrs,
                parent,
            )
        }
        _ => jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs),
    }
    .ok();
    let handle = || {
        if request.protocol_version != DAEMON_PROTOCOL_VERSION {
            return error_response(
                request.id,
                format!(
                    "unsupported daemon protocol {}; expected {}",
                    request.protocol_version, DAEMON_PROTOCOL_VERSION
                ),
            );
        }
        if request.build_id != build_id {
            return error_response(
                request.id,
                format!(
                    "daemon build mismatch: client {}; daemon {}",
                    request.build_id, build_id
                ),
            );
        }
        let id = request.id;
        match request.kind {
            DaemonRequestKind::Hello => DaemonResponse {
                id,
                kind: DaemonResponseKind::Hello {
                    protocol_version: DAEMON_PROTOCOL_VERSION,
                    build_id: build_id.to_owned(),
                    capabilities: Vec::new(),
                },
            },
            DaemonRequestKind::Status => DaemonResponse {
                id,
                kind: DaemonResponseKind::Status(DaemonStatus {
                    protocol_version: DAEMON_PROTOCOL_VERSION,
                    build_id: build_id.to_owned(),
                    pid: std::process::id(),
                    socket_path: layout.socket_path.clone(),
                    coredump_policy: coredump_policy.clone(),
                    adapters_enabled: if attention.muted() {
                        Vec::new()
                    } else {
                        vec!["attention".to_owned()]
                    },
                }),
            },
            DaemonRequestKind::TelemetryHealth => DaemonResponse {
                id,
                kind: DaemonResponseKind::TelemetryHealth(telemetry_health_report()),
            },
            DaemonRequestKind::AttentionSnapshot {
                container_name,
                panes,
            } => match attention.ingest_panes(&container_name, &panes) {
                Ok(notifications) => DaemonResponse {
                    id,
                    kind: DaemonResponseKind::AttentionAccepted {
                        notifications,
                        muted: attention.muted(),
                    },
                },
                Err(error) => error_response(id, error.to_string()),
            },
            DaemonRequestKind::Shutdown => DaemonResponse {
                id,
                kind: DaemonResponseKind::Shutdown { accepted: true },
            },
        }
    };
    let response = if let Some(operation) = operation.as_ref() {
        operation.span().in_scope(handle)
    } else {
        handle()
    };
    HandledResponse {
        response,
        operation,
    }
}

fn telemetry_health_report() -> TelemetryHealthReport {
    let health = jackin_diagnostics::telemetry_health_snapshot();
    let health_signal = |value: jackin_diagnostics::TelemetrySignalHealth| TelemetrySignalHealth {
        attempts: value.attempts,
        successes: value.successes,
        failures: value.failures,
    };
    let flush = match health.flush {
        jackin_diagnostics::TelemetryFlushStatus::Pending => TelemetryFlushStatus::Pending,
        jackin_diagnostics::TelemetryFlushStatus::Succeeded => TelemetryFlushStatus::Succeeded,
        jackin_diagnostics::TelemetryFlushStatus::Failed => TelemetryFlushStatus::Failed,
    };
    let (resolved, config_failure) = match jackin_diagnostics::resolved_otlp_config_fingerprint() {
        Ok(config) => (config, None),
        Err(failure) => (None, Some(telemetry_config_failure(failure))),
    };
    let config_signal =
        |value: jackin_diagnostics::OtlpSignalFingerprint| TelemetrySignalConfigFingerprint {
            authority: value.authority,
            tls: value.tls,
        };
    let (traces, logs, metrics, compression, sampler) = resolved.map_or_else(
        || {
            (
                None,
                None,
                None,
                "gzip".to_owned(),
                "parentbased_always_on".to_owned(),
            )
        },
        |config| {
            (
                Some(config_signal(config.traces)),
                Some(config_signal(config.logs)),
                Some(config_signal(config.metrics)),
                config.compression.to_owned(),
                config.sampler.to_owned(),
            )
        },
    );
    TelemetryHealthReport {
        fingerprint: SanitizedConfigFingerprint {
            traces,
            logs,
            metrics,
            compression,
            sampler,
            active_signals: health.active_signals,
            service_name: "jackin-daemon".to_owned(),
            app_mode: "daemon".to_owned(),
        },
        config_failure,
        health: TelemetryHealthSnapshot {
            active_signals: health.active_signals,
            traces: health_signal(health.traces),
            logs: health_signal(health.logs),
            metrics: health_signal(health.metrics),
            facade_rejections: health.facade_rejections,
            flush,
            shutdown_completed: health.shutdown_completed,
            shutdown_succeeded: health.shutdown_succeeded,
            shutdown_timed_out: health.shutdown_timed_out,
        },
    }
}

const fn telemetry_config_failure(
    failure: jackin_diagnostics::TelemetryConfigFailure,
) -> TelemetryConfigFailure {
    match failure {
        jackin_diagnostics::TelemetryConfigFailure::MissingSignalEndpoint => {
            TelemetryConfigFailure::MissingSignalEndpoint
        }
        jackin_diagnostics::TelemetryConfigFailure::UnsupportedProtocol => {
            TelemetryConfigFailure::UnsupportedProtocol
        }
        jackin_diagnostics::TelemetryConfigFailure::ConflictingSampler => {
            TelemetryConfigFailure::ConflictingSampler
        }
        jackin_diagnostics::TelemetryConfigFailure::UnsupportedCompression => {
            TelemetryConfigFailure::UnsupportedCompression
        }
        jackin_diagnostics::TelemetryConfigFailure::InvalidTimeout => {
            TelemetryConfigFailure::InvalidTimeout
        }
        jackin_diagnostics::TelemetryConfigFailure::InvalidHeaders => {
            TelemetryConfigFailure::InvalidHeaders
        }
        jackin_diagnostics::TelemetryConfigFailure::InvalidResourceAttributes => {
            TelemetryConfigFailure::InvalidResourceAttributes
        }
        jackin_diagnostics::TelemetryConfigFailure::InvalidEndpoint => {
            TelemetryConfigFailure::InvalidEndpoint
        }
        jackin_diagnostics::TelemetryConfigFailure::EmptyValue => {
            TelemetryConfigFailure::EmptyValue
        }
        jackin_diagnostics::TelemetryConfigFailure::IncompleteClientIdentity => {
            TelemetryConfigFailure::IncompleteClientIdentity
        }
    }
}

pub trait AttentionNotifierAdapter {
    fn ingest_panes(
        &mut self,
        container_name: &str,
        panes: &[AttentionPaneStatus],
    ) -> Result<usize>;
    fn muted(&self) -> bool;
}

impl<N: AttentionNotifier> AttentionNotifierAdapter for AttentionAdapter<N> {
    fn ingest_panes(
        &mut self,
        container_name: &str,
        panes: &[AttentionPaneStatus],
    ) -> Result<usize> {
        Self::ingest_panes(self, container_name, panes)
    }

    fn muted(&self) -> bool {
        Self::muted(self)
    }
}

fn read_response(stream: UnixStream) -> Result<DaemonResponse> {
    let mut line = String::new();
    BufReader::new(stream)
        .take(MAX_REQUEST_BYTES + 1)
        .read_line(&mut line)
        .context("reading daemon response")?;
    if line.len() as u64 > MAX_REQUEST_BYTES {
        bail!("daemon response exceeds {MAX_REQUEST_BYTES} byte limit");
    }
    serde_json::from_str(line.trim_end()).context("parsing daemon response")
}

fn error_response(id: impl Into<String>, message: impl Into<String>) -> DaemonResponse {
    DaemonResponse {
        id: id.into(),
        kind: DaemonResponseKind::Error {
            message: message.into(),
        },
    }
}

fn write_pid(layout: &DaemonLayout) -> Result<()> {
    ensure_run_dir(layout)?;
    fs::write(&layout.pid_path, format!("{}\n", std::process::id()))
        .with_context(|| format!("writing {}", layout.pid_path.display()))
}

fn cleanup_runtime_files(layout: &DaemonLayout) {
    drop(remove_if_exists(&layout.socket_path));
    drop(remove_if_exists(&layout.pid_path));
}

fn write_parented(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("removing {}", path.display())),
    }
}

fn disable_coredumps() -> CoredumpPolicy {
    CoredumpPolicy::Unsupported {
        residual_risk:
            "core dump suppression is not wired for this build; attention payloads are scrubbed"
                .to_owned(),
    }
}

pub fn notification_command_for_host(title: &str, body: &str) -> Option<NotificationCommand> {
    if cfg!(target_os = "macos") {
        Some(NotificationCommand {
            program: "osascript".to_owned(),
            args: vec![
                "-e".to_owned(),
                format!(
                    "display notification {} with title {}",
                    apple_script_string(body),
                    apple_script_string(title)
                ),
            ],
        })
    } else if cfg!(target_os = "linux") {
        Some(NotificationCommand {
            program: "notify-send".to_owned(),
            args: vec![title.to_owned(), body.to_owned()],
        })
    } else {
        None
    }
}

fn apple_script_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests;
