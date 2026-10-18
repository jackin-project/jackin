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
use jackin_workflow::{
    InstanceInventory, OperatorDecision, RecordedDispatch, RecordedReview, RecordedTest,
    RecoveryReport, RunDetail, RunSummary, WorkflowBackend,
};
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
    WorkflowStart {
        plan_hash: String,
        max_repair_turns: u32,
        idempotency_key: String,
    },
    WorkflowStatus {
        run_id: Option<String>,
    },
    WorkflowList {
        limit: u32,
    },
    WorkflowDetail {
        run_id: String,
        after_seq: i64,
        event_limit: u32,
    },
    WorkflowPause {
        run_id: String,
        reason: String,
        checkpoint: bool,
    },
    WorkflowResume {
        run_id: String,
    },
    WorkflowCancel {
        run_id: String,
        reason: String,
    },
    WorkflowResolve {
        run_id: String,
        command_id: String,
        decision: String,
        observed_session_state: String,
    },
    WorkflowRecordSession {
        run_id: String,
        expected_version: i64,
        worker: String,
        instance_id: Option<String>,
        capsule_epoch: Option<u64>,
        session_id: Option<u64>,
        process_identity: Option<String>,
        agent_profile: Option<String>,
        auth_profile: Option<String>,
    },
    WorkflowRecordCandidate {
        run_id: String,
        candidate: String,
        expected_version: i64,
    },
    WorkflowRecordCheckpoint {
        run_id: String,
        expected_version: i64,
        candidate: String,
        commit: String,
        tree: String,
        source_manifest: String,
        local_path: String,
        backup_state: String,
    },
    WorkflowRecordTest {
        run_id: String,
        expected_version: i64,
        check_program: String,
        check_script: String,
        bundle_digest: String,
        repo: Option<String>,
        scratch: String,
        timeout_secs: u64,
    },
    WorkflowRecordReview {
        run_id: String,
        expected_version: i64,
        review_file: String,
        policy_hash: String,
    },
    WorkflowRecordBackup {
        run_id: String,
        expected_version: i64,
        remote: String,
        git_ref: String,
    },
    WorkflowAdvance {
        run_id: String,
        expected_version: i64,
        actor: String,
        observation: String,
        expected_policy_hash: Option<String>,
    },
    WorkflowRecordDispatchIntent {
        run_id: String,
        session_ref: String,
        prompt: String,
        turn: u32,
        lease_generation: u64,
    },
    WorkflowRecover {
        inventory: InstanceInventory,
    },
}

impl DaemonRequestKind {
    const fn rpc_method(&self) -> &'static str {
        match self {
            Self::Hello => "jackin.host.Daemon/Hello",
            Self::Status => "jackin.host.Daemon/Status",
            Self::TelemetryHealth => "jackin.host.Daemon/TelemetryHealth",
            Self::AttentionSnapshot { .. } => "jackin.host.Daemon/AttentionSnapshot",
            Self::Shutdown => "jackin.host.Daemon/Shutdown",
            Self::WorkflowStart { .. } => "jackin.host.Daemon/WorkflowStart",
            Self::WorkflowStatus { .. } => "jackin.host.Daemon/WorkflowStatus",
            Self::WorkflowList { .. } => "jackin.host.Daemon/WorkflowList",
            Self::WorkflowDetail { .. } => "jackin.host.Daemon/WorkflowDetail",
            Self::WorkflowPause { .. } => "jackin.host.Daemon/WorkflowPause",
            Self::WorkflowResume { .. } => "jackin.host.Daemon/WorkflowResume",
            Self::WorkflowCancel { .. } => "jackin.host.Daemon/WorkflowCancel",
            Self::WorkflowResolve { .. } => "jackin.host.Daemon/WorkflowResolve",
            Self::WorkflowRecordSession { .. } => "jackin.host.Daemon/WorkflowRecordSession",
            Self::WorkflowRecordCandidate { .. } => "jackin.host.Daemon/WorkflowRecordCandidate",
            Self::WorkflowRecordCheckpoint { .. } => "jackin.host.Daemon/WorkflowRecordCheckpoint",
            Self::WorkflowRecordTest { .. } => "jackin.host.Daemon/WorkflowRecordTest",
            Self::WorkflowRecordReview { .. } => "jackin.host.Daemon/WorkflowRecordReview",
            Self::WorkflowRecordBackup { .. } => "jackin.host.Daemon/WorkflowRecordBackup",
            Self::WorkflowAdvance { .. } => "jackin.host.Daemon/WorkflowAdvance",
            Self::WorkflowRecordDispatchIntent { .. } => {
                "jackin.host.Daemon/WorkflowRecordDispatchIntent"
            }
            Self::WorkflowRecover { .. } => "jackin.host.Daemon/WorkflowRecover",
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
    WorkflowRun {
        summary: RunSummary,
    },
    WorkflowRuns {
        summaries: Vec<RunSummary>,
    },
    WorkflowRunDetail {
        detail: RunDetail,
    },
    WorkflowRecord {
        summary: RunSummary,
    },
    WorkflowTestRecord {
        summary: RunSummary,
        test: RecordedTest,
    },
    WorkflowReviewRecord {
        summary: RunSummary,
        review: RecordedReview,
    },
    WorkflowDispatchIntent {
        summary: RunSummary,
        dispatch: RecordedDispatch,
    },
    WorkflowRecovery {
        report: RecoveryReport,
    },
    Error {
        message: String,
        /// C7 typed error code (`invalid_request`, `not_found`,
        /// `busy`, `denied`, `state_conflict`, `needs_operator`,
        /// `execution_error`). Defaults for readers of older daemon
        /// responses that carry no code.
        #[serde(default)]
        code: String,
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
        let status = jackin_runtime_process_telemetry::process_telemetry::exec_sync(&request)
            .context("dispatching notification command")?;
        if status.success {
            Ok(())
        } else {
            bail!("notification command exited unsuccessfully")
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
    /// Digest-verified plan store the daemon cross-checks frozen
    /// test bindings against before recording a verdict.
    pub plan_store_dir: PathBuf,
}

impl DaemonLayout {
    #[must_use]
    pub fn new(paths: &JackinPaths) -> Self {
        let run_dir = paths.jackin_home.join("run");
        Self {
            socket_path: run_dir.join(SOCKET_FILE_NAME),
            pid_path: run_dir.join(PID_FILE_NAME),
            run_dir,
            plan_store_dir: paths.data_dir.join("workflow-plans"),
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
    match jackin_workflow::WorkflowService::open(
        &layout.run_dir.join("workflow-runs.db"),
        &layout.run_dir.join("workflow-artifacts"),
    ) {
        Ok(mut workflow) => serve_with_workflow(layout, build_id, &mut workflow),
        Err(error) => serve_with_workflow(
            layout,
            build_id,
            &mut jackin_workflow::UnavailableBackend::new(error.to_string()),
        ),
    }
}

/// Serve with an injected workflow backend (trait-param injection for
/// run control, mirroring [`AttentionNotifierAdapter`]). Opening the
/// service persists the incremented owner epoch, so every `serve()`
/// startup invalidates automatic input stamped with older epochs.
///
/// This blocks the calling thread (synchronous socket loop plus
/// blocking store calls). Callers on an async runtime must move it to
/// a blocking thread instead of stalling the executor.
pub fn serve_with_workflow(
    layout: &DaemonLayout,
    build_id: &str,
    workflow: &mut dyn WorkflowBackend,
) -> Result<ServeOutcome> {
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    let listener = match bind_control_socket(layout).and_then(|listener| {
        write_pid(layout)?;
        Ok(listener)
    }) {
        Ok(listener) => listener,
        Err(error) => {
            jackin_telemetry::stream::complete_error(
                open,
                jackin_telemetry::schema::enums::ErrorType::IoError,
            );
            return Err(error);
        }
    };
    jackin_telemetry::stream::complete_success(open);
    // Automatic recovery on daemon startup: the owner epoch was
    // already bumped when the workflow service opened, so run
    // `recover` now against the startup inventory before accepting
    // clients. Crash-left `dispatching` commands become
    // `delivery_unknown` here and are never resubmitted.
    //
    // Fail-open rationale: recovery is re-runnable through the
    // explicit `WorkflowRecover` RPC, and refusing to serve would
    // hide the store behind a dead socket with no operator path to
    // inspect it. A startup failure is logged and serving continues;
    // per-run storage failures are still reported by the recovery
    // report itself and by later RPC errors.
    startup_recovery(workflow, &InstanceInventory::default());
    let close = jackin_telemetry::stream::close_on_drop();
    let coredump_policy = disable_coredumps();
    let attention_enabled = std::env::var_os("JACKIN_ATTENTION").is_some_and(|value| value == "1");
    let mut attention = AttentionAdapter::new(HostAttentionNotifier::new(
        StdNotificationDispatcher,
        attention_enabled,
    ));
    let result = (|| {
        for stream in listener.incoming() {
            let mut stream = stream.context("accepting daemon client")?;
            let response = handle_stream(
                &mut stream,
                layout,
                build_id,
                &coredump_policy,
                &mut attention,
                workflow,
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
    })();
    match &result {
        Ok(_) => close.complete_success(),
        Err(_) => close.complete_error(jackin_telemetry::schema::enums::ErrorType::IoError),
    }
    result
}

/// Run one recovery pass and return its report, or `None` when
/// recovery itself fails. Extracted so unit tests can prove both
/// outcomes without binding the control socket: `Some` means the
/// startup pass ran, `None` means the caller logs and keeps serving.
pub fn startup_recovery(
    workflow: &mut dyn WorkflowBackend,
    inventory: &InstanceInventory,
) -> Option<RecoveryReport> {
    match workflow.recover(inventory) {
        Ok(report) => Some(report),
        Err(error) => {
            use std::io::Write as _;
            drop(writeln!(
                std::io::stderr().lock(),
                "daemon startup recovery failed: {error}; serving continues"
            ));
            None
        }
    }
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
            | (
                "jackin.host.Daemon/WorkflowStart",
                DaemonResponseKind::WorkflowRun { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowStatus",
                DaemonResponseKind::WorkflowRun { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowList",
                DaemonResponseKind::WorkflowRuns { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowDetail",
                DaemonResponseKind::WorkflowRunDetail { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowPause",
                DaemonResponseKind::WorkflowRun { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowResume",
                DaemonResponseKind::WorkflowRun { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowCancel",
                DaemonResponseKind::WorkflowRun { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowResolve",
                DaemonResponseKind::WorkflowRun { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecordSession",
                DaemonResponseKind::WorkflowRecord { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecordCandidate",
                DaemonResponseKind::WorkflowRecord { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecordCheckpoint",
                DaemonResponseKind::WorkflowRecord { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecordTest",
                DaemonResponseKind::WorkflowTestRecord { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecordReview",
                DaemonResponseKind::WorkflowReviewRecord { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecordBackup",
                DaemonResponseKind::WorkflowRecord { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowAdvance",
                DaemonResponseKind::WorkflowRun { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecordDispatchIntent",
                DaemonResponseKind::WorkflowDispatchIntent { .. }
            )
            | (
                "jackin.host.Daemon/WorkflowRecover",
                DaemonResponseKind::WorkflowRecovery { .. }
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
    workflow: &mut dyn WorkflowBackend,
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
            workflow,
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
    let mut workflow = jackin_workflow::UnavailableBackend::new("no workflow service on this path");
    handle_request_line_inner(
        line,
        layout,
        build_id,
        coredump_policy,
        attention,
        &mut workflow,
    )
    .complete_without_transport()
}

/// Request-line entry point with an injected workflow backend for
/// run control.
pub fn handle_request_line_with_workflow(
    line: &str,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
    attention: &mut impl AttentionNotifierAdapter,
    workflow: &mut dyn WorkflowBackend,
) -> DaemonResponse {
    handle_request_line_inner(line, layout, build_id, coredump_policy, attention, workflow)
        .complete_without_transport()
}

fn handle_request_line_inner(
    line: &str,
    layout: &DaemonLayout,
    build_id: &str,
    coredump_policy: &CoredumpPolicy,
    attention: &mut impl AttentionNotifierAdapter,
    workflow: &mut dyn WorkflowBackend,
) -> HandledResponse {
    if line.is_empty() {
        return HandledResponse::without_operation(error_response(
            "unknown",
            "empty daemon request",
        ));
    }
    match serde_json::from_str::<DaemonRequest>(line) {
        Ok(request) => handle_request(
            request,
            layout,
            build_id,
            coredump_policy,
            attention,
            workflow,
        ),
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
    workflow: &mut dyn WorkflowBackend,
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
            kind @ (DaemonRequestKind::WorkflowStart { .. }
            | DaemonRequestKind::WorkflowStatus { .. }
            | DaemonRequestKind::WorkflowList { .. }
            | DaemonRequestKind::WorkflowDetail { .. }
            | DaemonRequestKind::WorkflowPause { .. }
            | DaemonRequestKind::WorkflowResume { .. }
            | DaemonRequestKind::WorkflowCancel { .. }
            | DaemonRequestKind::WorkflowResolve { .. }
            | DaemonRequestKind::WorkflowRecordSession { .. }
            | DaemonRequestKind::WorkflowRecordCandidate { .. }
            | DaemonRequestKind::WorkflowRecordCheckpoint { .. }
            | DaemonRequestKind::WorkflowRecordTest { .. }
            | DaemonRequestKind::WorkflowRecordReview { .. }
            | DaemonRequestKind::WorkflowRecordBackup { .. }
            | DaemonRequestKind::WorkflowAdvance { .. }
            | DaemonRequestKind::WorkflowRecordDispatchIntent { .. }
            | DaemonRequestKind::WorkflowRecover { .. }) => {
                handle_workflow_request(id, kind, &layout.plan_store_dir, workflow)
            }
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
    coded_error_response(id, "execution_error", message)
}

fn coded_error_response(
    id: impl Into<String>,
    code: &str,
    message: impl Into<String>,
) -> DaemonResponse {
    DaemonResponse {
        id: id.into(),
        kind: DaemonResponseKind::Error {
            message: message.into(),
            code: code.to_owned(),
        },
    }
}

/// C7 typed error code for a workflow failure kind.
fn workflow_error_code(error: &jackin_workflow::WorkflowError) -> &'static str {
    use jackin_workflow::ErrorKind;
    match error.kind() {
        ErrorKind::Invalid => "invalid_request",
        ErrorKind::NotFound => "not_found",
        ErrorKind::Busy => "busy",
        ErrorKind::StaleVersion | ErrorKind::StaleEpoch => "state_conflict",
        ErrorKind::Blocked => "needs_operator",
        ErrorKind::Storage => "execution_error",
    }
}

/// Verify `record_test` inputs against the run's stored plan
/// before the daemon freezes anything: the full check argv must match
/// the supported frozen shape, script bytes must match the selected
/// bundle entry, the bundle digest must match the frozen manifest,
/// and timeout must match the plan. The CLI checks the program early
/// for a fast failure;
/// this daemon-side check is authoritative for every RPC caller,
/// including direct socket clients that bypass the CLI. On success
/// the stored per-file bundle manifest is returned to the service.
fn check_test_binding(
    workflow: &mut dyn WorkflowBackend,
    plan_store_dir: &Path,
    run_id: &str,
    check_program: &str,
    check_script: &str,
    bundle_digest: &str,
    timeout_secs: u64,
) -> Result<Vec<jackin_workflow_spec::model::TestBundleFile>, jackin_workflow::WorkflowError> {
    let summary = workflow.run_status(Some(run_id))?;
    let store = jackin_workflow_spec::store::InputStore::open(plan_store_dir).map_err(|error| {
        jackin_workflow::WorkflowError::invalid(format!(
            "cannot verify the test binding for run `{run_id}`: {error}"
        ))
    })?;
    let plan = store.read_plan(&summary.plan_hash).map_err(|error| {
        jackin_workflow::WorkflowError::invalid(format!(
            "cannot verify the test binding for run `{run_id}`: {error}"
        ))
    })?;
    verify_test_binding(
        &plan.verification,
        check_program,
        check_script,
        bundle_digest,
        timeout_secs,
        &store,
    )?;
    Ok(plan.verification.test_bundle_files.clone())
}

/// Binding comparison against the frozen input store.
fn verify_test_binding(
    verification: &jackin_workflow_spec::model::VerificationPolicy,
    check_program: &str,
    check_script: &str,
    bundle_digest: &str,
    timeout_secs: u64,
    store: &jackin_workflow_spec::store::InputStore,
) -> Result<(), jackin_workflow::WorkflowError> {
    let argv = &verification.check_argv;
    if argv.len() != 3
        || argv[0] != check_program
        || argv[2] != "/work/repo"
        || !argv[1].starts_with(jackin_core::container_paths::CHECKS_PREFIX)
    {
        return Err(jackin_workflow::WorkflowError::invalid(format!(
            "check argv does not match the supported frozen shape [program, /jackin/checks/<manifest path>, /work/repo]: {argv:?}"
        )));
    }
    let logical_path = &argv[1][jackin_core::container_paths::CHECKS_PREFIX.len()..];
    let logical_path = Path::new(logical_path);
    if logical_path.as_os_str().is_empty()
        || logical_path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || !Path::new(check_script).ends_with(logical_path)
    {
        return Err(jackin_workflow::WorkflowError::invalid(format!(
            "supplied check script path `{check_script}` does not match frozen bundle path `{}`",
            logical_path.display()
        )));
    }
    let logical_path = logical_path.to_string_lossy();
    let file = verification
        .test_bundle_files
        .iter()
        .find(|file| file.logical_path == logical_path)
        .ok_or_else(|| {
            jackin_workflow::WorkflowError::invalid(format!(
                "frozen check script `{logical_path}` is not a member of the selected test bundle"
            ))
        })?;
    let frozen_bytes = store.read_input(&file.digest).map_err(|error| {
        jackin_workflow::WorkflowError::invalid(format!(
            "cannot read frozen check script `{logical_path}`: {error}"
        ))
    })?;
    let supplied_bytes = fs::read(check_script).map_err(|error| {
        jackin_workflow::WorkflowError::invalid(format!(
            "cannot read supplied check script `{check_script}`: {error}"
        ))
    })?;
    if supplied_bytes != frozen_bytes {
        return Err(jackin_workflow::WorkflowError::invalid(format!(
            "supplied check script bytes do not match frozen bundle entry `{logical_path}`"
        )));
    }
    if verification.check_image_digest != jackin_workflow::TEST_SANDBOX_IMAGE {
        return Err(jackin_workflow::WorkflowError::invalid(format!(
            "check image `{}` does not match the supported sandbox image `{}`",
            verification.check_image_digest,
            jackin_workflow::TEST_SANDBOX_IMAGE
        )));
    }
    if verification.test_bundle_digest != bundle_digest {
        return Err(jackin_workflow::WorkflowError::invalid(
            "check bundle digest does not match the frozen plan bundle".to_owned(),
        ));
    }
    if verification.timeout_seconds != timeout_secs {
        return Err(jackin_workflow::WorkflowError::invalid(format!(
            "check timeout {timeout_secs}s does not match the frozen plan timeout {}s",
            verification.timeout_seconds
        )));
    }
    Ok(())
}

/// Run-control dispatch: every variant drives the injected
/// workflow backend as the `operator` actor. `plan_store_dir` is the
/// digest-verified plan store `record_test` bindings are checked
/// against before any verdict is frozen.
fn handle_workflow_request(
    id: String,
    kind: DaemonRequestKind,
    plan_store_dir: &Path,
    workflow: &mut dyn WorkflowBackend,
) -> DaemonResponse {
    match kind {
        DaemonRequestKind::WorkflowStart {
            plan_hash,
            max_repair_turns,
            idempotency_key,
        } => workflow_response(
            id,
            workflow.start_run(&plan_hash, max_repair_turns, &idempotency_key),
        ),
        DaemonRequestKind::WorkflowStatus { run_id } => {
            workflow_response(id, workflow.run_status(run_id.as_deref()))
        }
        DaemonRequestKind::WorkflowList { limit } => match workflow.list_runs(limit) {
            Ok(summaries) => DaemonResponse {
                id,
                kind: DaemonResponseKind::WorkflowRuns { summaries },
            },
            Err(error) => {
                let code = workflow_error_code(&error);
                coded_error_response(id, code, error.to_string())
            }
        },
        DaemonRequestKind::WorkflowDetail {
            run_id,
            after_seq,
            event_limit,
        } => match workflow.run_detail(&run_id, after_seq, event_limit) {
            Ok(detail) => DaemonResponse {
                id,
                kind: DaemonResponseKind::WorkflowRunDetail { detail },
            },
            Err(error) => {
                let code = workflow_error_code(&error);
                coded_error_response(id, code, error.to_string())
            }
        },
        DaemonRequestKind::WorkflowPause {
            run_id,
            reason,
            checkpoint,
        } => workflow_response(
            id,
            workflow.pause_run(&run_id, "operator", &reason, checkpoint),
        ),
        DaemonRequestKind::WorkflowResume { run_id } => {
            workflow_response(id, workflow.resume_run(&run_id, "operator"))
        }
        DaemonRequestKind::WorkflowCancel { run_id, reason } => {
            workflow_response(id, workflow.cancel_run(&run_id, "operator", &reason))
        }
        DaemonRequestKind::WorkflowResolve {
            run_id,
            command_id,
            decision,
            observed_session_state,
        } => match OperatorDecision::parse(&decision) {
            Some(parsed) => workflow_response(
                id,
                workflow.resolve_input(
                    &run_id,
                    &command_id,
                    parsed,
                    "operator",
                    &observed_session_state,
                ),
            ),
            None => coded_error_response(
                id,
                "invalid_request",
                format!("unknown operator decision `{decision}`"),
            ),
        },
        kind @ (DaemonRequestKind::WorkflowRecordSession { .. }
        | DaemonRequestKind::WorkflowRecordCandidate { .. }
        | DaemonRequestKind::WorkflowRecordCheckpoint { .. }
        | DaemonRequestKind::WorkflowRecordTest { .. }
        | DaemonRequestKind::WorkflowRecordReview { .. }
        | DaemonRequestKind::WorkflowRecordBackup { .. }
        | DaemonRequestKind::WorkflowRecordDispatchIntent { .. }
        | DaemonRequestKind::WorkflowAdvance { .. }) => {
            handle_workflow_record_request(id, kind, plan_store_dir, workflow)
        }
        DaemonRequestKind::WorkflowRecover { inventory } => match workflow.recover(&inventory) {
            Ok(report) => DaemonResponse {
                id,
                kind: DaemonResponseKind::WorkflowRecovery { report },
            },
            Err(error) => {
                let code = workflow_error_code(&error);
                coded_error_response(id, code, error.to_string())
            }
        },
        _ => coded_error_response(id, "invalid_request", "not a workflow request"),
    }
}

fn handle_workflow_record_request(
    id: String,
    kind: DaemonRequestKind,
    plan_store_dir: &Path,
    workflow: &mut dyn WorkflowBackend,
) -> DaemonResponse {
    match kind {
        DaemonRequestKind::WorkflowRecordSession {
            run_id,
            expected_version,
            worker,
            instance_id,
            capsule_epoch,
            session_id,
            process_identity,
            agent_profile,
            auth_profile,
        } => workflow_record_response(
            id,
            workflow.record_session(
                &run_id,
                expected_version,
                &worker,
                instance_id.as_deref(),
                capsule_epoch,
                session_id,
                process_identity.as_deref(),
                agent_profile.as_deref(),
                auth_profile.as_deref(),
            ),
        ),
        DaemonRequestKind::WorkflowRecordCandidate {
            run_id,
            candidate,
            expected_version,
        } => workflow_record_response(
            id,
            workflow.record_candidate(&run_id, &candidate, expected_version),
        ),
        DaemonRequestKind::WorkflowRecordCheckpoint {
            run_id,
            expected_version,
            candidate,
            commit,
            tree,
            source_manifest,
            local_path,
            backup_state,
        } => workflow_record_response(
            id,
            workflow.record_checkpoint(
                &run_id,
                expected_version,
                &candidate,
                &commit,
                &tree,
                &source_manifest,
                &local_path,
                &backup_state,
            ),
        ),
        DaemonRequestKind::WorkflowRecordTest {
            run_id,
            expected_version,
            check_program,
            check_script,
            bundle_digest,
            repo,
            scratch,
            timeout_secs,
        } => match check_test_binding(
            workflow,
            plan_store_dir,
            &run_id,
            &check_program,
            &check_script,
            &bundle_digest,
            timeout_secs,
        )
        .and_then(|bundle_files| {
            workflow.record_test(
                &run_id,
                expected_version,
                &check_program,
                &check_script,
                &bundle_digest,
                &bundle_files,
                repo.as_deref(),
                &scratch,
                timeout_secs,
            )
        }) {
            Ok((summary, test)) => DaemonResponse {
                id,
                kind: DaemonResponseKind::WorkflowTestRecord { summary, test },
            },
            Err(error) => {
                let code = workflow_error_code(&error);
                coded_error_response(id, code, error.to_string())
            }
        },
        DaemonRequestKind::WorkflowRecordReview {
            run_id,
            expected_version,
            review_file,
            policy_hash,
        } => match workflow.record_review(&run_id, expected_version, &review_file, &policy_hash) {
            Ok((summary, review)) => DaemonResponse {
                id,
                kind: DaemonResponseKind::WorkflowReviewRecord { summary, review },
            },
            Err(error) => {
                let code = workflow_error_code(&error);
                coded_error_response(id, code, error.to_string())
            }
        },
        DaemonRequestKind::WorkflowRecordBackup {
            run_id,
            expected_version,
            remote,
            git_ref,
        } => workflow_record_response(
            id,
            workflow.record_backup(&run_id, expected_version, &remote, &git_ref),
        ),
        DaemonRequestKind::WorkflowAdvance {
            run_id,
            expected_version,
            actor,
            observation,
            expected_policy_hash,
        } => workflow_response(
            id,
            workflow.advance_run(
                &run_id,
                expected_version,
                &actor,
                &observation,
                expected_policy_hash.as_deref(),
            ),
        ),
        DaemonRequestKind::WorkflowRecordDispatchIntent {
            run_id,
            session_ref,
            prompt,
            turn,
            lease_generation,
        } => workflow_dispatch_response(
            id,
            workflow.record_dispatch_intent(&run_id, &session_ref, &prompt, turn, lease_generation),
        ),
        _ => coded_error_response(id, "invalid_request", "not a workflow record request"),
    }
}

fn workflow_dispatch_response(
    id: String,
    result: Result<(RunSummary, RecordedDispatch), jackin_workflow::WorkflowError>,
) -> DaemonResponse {
    match result {
        Ok((summary, dispatch)) => DaemonResponse {
            id,
            kind: DaemonResponseKind::WorkflowDispatchIntent { summary, dispatch },
        },
        Err(error) => {
            let code = workflow_error_code(&error);
            coded_error_response(id, code, error.to_string())
        }
    }
}

fn workflow_record_response(
    id: String,
    result: Result<RunSummary, jackin_workflow::WorkflowError>,
) -> DaemonResponse {
    match result {
        Ok(summary) => DaemonResponse {
            id,
            kind: DaemonResponseKind::WorkflowRecord { summary },
        },
        Err(error) => {
            let code = workflow_error_code(&error);
            coded_error_response(id, code, error.to_string())
        }
    }
}

fn workflow_response(
    id: String,
    result: Result<RunSummary, jackin_workflow::WorkflowError>,
) -> DaemonResponse {
    match result {
        Ok(summary) => DaemonResponse {
            id,
            kind: DaemonResponseKind::WorkflowRun { summary },
        },
        Err(error) => {
            let code = workflow_error_code(&error);
            coded_error_response(id, code, error.to_string())
        }
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
            args: linux_notify_args(title, body),
        })
    } else {
        None
    }
}

fn linux_notify_args(title: &str, body: &str) -> Vec<String> {
    fn sanitize(value: &str) -> String {
        value
            .chars()
            .filter(|character| !character.is_control())
            .take(200)
            .collect()
    }

    vec!["--".to_owned(), sanitize(title), sanitize(body)]
}

fn apple_script_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests;
