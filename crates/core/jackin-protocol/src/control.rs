// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

use crate::TelemetryContext;
use crate::agent_status::AgentStatusReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlRequest {
    pub ctx: TelemetryContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_capability: Option<String>,
    pub msg: ClientMsg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    TelemetryHealth,
    Status,
    Snapshot,
    Agents,
    ReportRuntimeEvent {
        session_id: u64,
        source_id: String,
        runtime: String,
        event: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<String>,
    },
    StatusCapture {
        session_id: u64,
    },
    UsageFocused,
    UsageRefreshFocused,
    UsageAccountList,
    ExecCommand {
        command: String,
        args: Vec<String>,
    },
    TokenUsage {
        session_id: u64,
    },
    SessionSend {
        session: u64,
        text: String,
    },
    TakeControl {
        session: u64,
    },
    ReleaseControl {
        session: u64,
    },
    Events {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<u64>,
    },
    #[serde(other)]
    Unknown,
}

impl ClientMsg {
    #[must_use]
    pub const fn rpc_method(&self) -> &'static str {
        match self {
            Self::Status => "jackin.capsule.Control/Status",
            Self::TelemetryHealth => "jackin.capsule.Control/TelemetryHealth",
            Self::Snapshot => "jackin.capsule.Control/Snapshot",
            Self::Agents => "jackin.capsule.Control/Agents",
            Self::ReportRuntimeEvent { .. } => "jackin.capsule.Control/ReportRuntimeEvent",
            Self::StatusCapture { .. } => "jackin.capsule.Control/StatusCapture",
            Self::UsageFocused => "jackin.capsule.Control/UsageFocused",
            Self::UsageRefreshFocused => "jackin.capsule.Control/UsageRefreshFocused",
            Self::UsageAccountList => "jackin.capsule.Control/UsageAccountList",
            Self::ExecCommand { .. } => "jackin.capsule.Control/ExecCommand",
            Self::TokenUsage { .. } => "jackin.capsule.Control/TokenUsage",
            Self::SessionSend { .. } => "jackin.capsule.Control/SessionSend",
            Self::TakeControl { .. } => "jackin.capsule.Control/TakeControl",
            Self::ReleaseControl { .. } => "jackin.capsule.Control/ReleaseControl",
            Self::Events { .. } => "jackin.capsule.Control/Events",
            Self::Unknown => "jackin.capsule.Control/Unknown",
        }
    }

    #[must_use]
    pub const fn is_subscription(&self) -> bool {
        matches!(self, Self::Events { .. })
    }
}

impl ServerMsg {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionList { .. } => "SessionList",
            Self::TelemetryHealth { .. } => "TelemetryHealth",
            Self::Snapshot { .. } => "Snapshot",
            Self::AgentRegistry { .. } => "AgentRegistry",
            Self::UsageFocused { .. } => "UsageFocused",
            Self::UsageAccounts { .. } => "UsageAccounts",
            Self::ExecResult { .. } => "ExecResult",
            Self::ExecDenied { .. } => "ExecDenied",
            Self::TokenUsage { .. } => "TokenUsage",
            Self::SessionSent { .. } => "SessionSent",
            Self::SessionSendDenied { .. } => "SessionSendDenied",
            Self::SessionEvent { .. } => "SessionEvent",
            Self::Ack => "Ack",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    TelemetryHealth {
        report: Box<TelemetryHealthReport>,
    },
    SessionList {
        sessions: Vec<SessionInfo>,
    },
    Snapshot {
        tabs: Vec<TabSnapshot>,
        active_tab: u32,
    },
    AgentRegistry {
        records: Vec<AgentRegistryEntry>,
    },
    Ack,
    UsageFocused {
        usage: Box<FocusedUsageView>,
    },
    UsageAccounts {
        accounts: Vec<AccountUsageSnapshotView>,
    },
    ExecResult {
        exit_code: i32,
        stdout: String,
        stderr: String,
        redacted_count: u32,
    },
    ExecDenied {
        reason: String,
    },
    TokenUsage {
        summary: Option<TokenUsageSummary>,
    },
    SessionSent {
        session: u64,
        bytes: u64,
    },
    SessionSendDenied {
        session: u64,
        reason: SessionSendRejection,
    },
    SessionEvent {
        event: Box<SessionEventRecord>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionSendRejection {
    UnknownSession,
    WriterClosed,
    OperatorOwned,
}

impl SessionSendRejection {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::UnknownSession => "no such session",
            Self::WriterClosed => "session PTY writer has exited",
            Self::OperatorOwned => "session input is owned by an operator",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionEventRecord {
    pub seq: u64,
    pub session: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub state: AgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_output_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_input_ms: Option<u64>,
    pub kind: SessionEventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionEventKind {
    Subscribed,
    StateChanged {
        previous: AgentState,
    },
    Activity,
    Exited {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

impl SessionEventKind {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Subscribed => "subscribed",
            Self::StateChanged { .. } => "state_changed",
            Self::Activity => "activity",
            Self::Exited { .. } => "exited",
        }
    }
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
    pub capsule_export: CapsuleExportCoverage,
    pub flush: TelemetryFlushStatus,
    pub shutdown_completed: bool,
    pub shutdown_succeeded: bool,
    pub shutdown_timed_out: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapsuleExportCoverage {
    Enabled,
    DisabledNoEndpoint,
    DisabledNetworkNone,
    DisabledUnclassifiedEndpoint,
    DisabledUnclassifiedAuth,
    NotApplicable,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountUsageSnapshotView {
    pub provider: String,
    pub account_label: String,
    pub source: String,
    pub confidence: String,
    pub window_kind: String,
    pub used_amount: Option<i64>,
    pub used_unit: Option<String>,
    pub limit_amount: Option<i64>,
    pub limit_unit: Option<String>,
    pub resets_at: Option<i64>,
    pub fetched_at: i64,
    pub expires_at: Option<i64>,
    pub status: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedUsageView {
    pub focused_agent: Option<String>,
    pub focused_provider: Option<String>,
    pub account: FocusedAccountHeader,
    pub buckets: Vec<QuotaBucketView>,
    pub status: UsageSnapshotStatus,
    pub source: UsageSource,
    pub confidence: UsageConfidence,
    pub fetched_at_epoch: i64,
    pub updated_label: String,
    pub status_bar_label: String,
    pub tabs: Vec<UsageProviderTab>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageActivityKind {
    Idle,
    Updating,
    Exceptional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageIdentityPresentation {
    pub provider_title: String,
    pub account_label: String,
    pub activity_label: String,
    pub activity_kind: UsageActivityKind,
    pub accessibility_label: String,
}

impl FocusedUsageView {
    #[must_use]
    pub fn unavailable(reason: impl Into<String>, now_epoch: i64) -> Self {
        let reason = reason.into();
        Self {
            focused_agent: None,
            focused_provider: None,
            account: FocusedAccountHeader {
                provider_label: "Usage".to_owned(),
                account_label: reason.clone(),
                username: None,
                plan_label: None,
                credential_origin: None,
            },
            buckets: Vec::new(),
            status: UsageSnapshotStatus::Unavailable,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            fetched_at_epoch: now_epoch,
            updated_label: "Unavailable".to_owned(),
            status_bar_label: "usage unavailable".to_owned(),
            tabs: Vec::new(),
            last_error: Some(reason),
        }
    }

    #[must_use]
    pub fn refreshing(provider: Option<&str>, now_epoch: i64) -> Self {
        let mut view = Self::unavailable("refreshing", now_epoch);
        view.focused_provider = provider.map(str::to_owned);
        view.account.provider_label.clear();
        view.account
            .provider_label
            .push_str(provider.unwrap_or("Usage"));
        view.account.account_label = String::new();
        view.status_bar_label.clear();
        view.status_bar_label.push_str("refreshing");
        view.updated_label.clear();
        view.updated_label.push_str("Refreshing");
        view
    }

    #[must_use]
    pub fn is_refreshing_placeholder(&self) -> bool {
        self.status == UsageSnapshotStatus::Unavailable
            && self.source == UsageSource::None
            && self.confidence == UsageConfidence::None
            && self.buckets.is_empty()
            && self.account.account_label.is_empty()
            && self.account.username.is_none()
            && self.account.plan_label.is_none()
            && self.account.credential_origin.is_none()
            && self.status_bar_label == "refreshing"
            && self.updated_label == "Refreshing"
            && self.last_error.as_deref() == Some("refreshing")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedAccountHeader {
    pub provider_label: String,
    pub account_label: String,
    #[serde(default)]
    pub username: Option<String>,
    pub plan_label: Option<String>,
    #[serde(default)]
    pub credential_origin: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusSlot {
    Session,
    Daily,
    Weekly,
    Spend,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Money {
    pub amount_minor: i64,
    pub currency: String,
    pub exponent: u8,
}

impl Money {
    #[must_use]
    pub fn new(amount_minor: i64, currency: impl Into<String>, exponent: u8) -> Self {
        Self {
            amount_minor,
            currency: currency.into(),
            exponent,
        }
    }

    #[must_use]
    pub fn major(&self) -> f64 {
        self.amount_minor as f64 / 10f64.powi(i32::from(self.exponent))
    }

    #[must_use]
    pub fn format_compact(&self) -> String {
        self.format_with_precision(0)
    }

    #[must_use]
    pub fn major_amount(&self) -> i64 {
        self.major().round() as i64
    }

    #[must_use]
    pub fn raw_percent_of(&self, cap: &Money) -> Option<i32> {
        if self.currency != cap.currency || self.exponent != cap.exponent || cap.amount_minor <= 0 {
            return None;
        }
        let scaled = self.amount_minor.saturating_mul(100);
        let raw = scaled.checked_div(cap.amount_minor)?;
        Some(i32::try_from(raw).unwrap_or(if raw < 0 { i32::MIN } else { i32::MAX }))
    }

    fn format_with_precision(&self, prec: usize) -> String {
        let value = self.major();
        match self.currency.as_str() {
            "USD" => format!("${value:.prec$}"),
            code if code.len() == 3 && code.chars().all(|c| c.is_ascii_uppercase()) => {
                format!("{code} {value:.prec$}")
            }
            other => format!("{value:.prec$} {other}"),
        }
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format_with_precision(usize::from(self.exponent)))
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSeverity {
    #[default]
    Normal,
    Warn,
    Danger,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaBucketView {
    pub label: String,
    pub used_label: Option<String>,
    pub limit_label: Option<String>,
    pub remaining_percent: Option<u8>,
    pub reset_label: Option<String>,
    #[serde(default)]
    pub resets_at: Option<i64>,
    #[serde(default)]
    pub status_slot: Option<StatusSlot>,
    pub pace_label: Option<String>,
    pub status: UsageSnapshotStatus,
    #[serde(default)]
    pub used_money: Option<Money>,
    #[serde(default)]
    pub limit_money: Option<Money>,
    #[serde(default)]
    pub severity: UsageSeverity,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsagePresentationLine {
    pub leading: Option<String>,
    pub trailing: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageDetailRowKind {
    Metadata,
    Bucket,
    Detail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageDetailRow {
    pub row_id: String,
    pub kind: UsageDetailRowKind,
    pub label: String,
    pub layout_lines: Vec<UsagePresentationLine>,
    pub display_label: String,
    pub meter_percent: Option<u8>,
    pub severity: UsageSeverity,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageDetailPresentation {
    pub rows: Vec<UsageDetailRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageProviderTab {
    #[serde(default)]
    pub id: String,
    pub label: String,
    pub status_label: String,
    pub account_label: String,
    pub plan_label: Option<String>,
    pub source_label: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSnapshotStatus {
    Fresh,
    Stale,
    NeedsLogin,
    NeedsSecret,
    Unsupported,
    Unavailable,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    ProviderApi,
    Cli,
    LocalLogs,
    Cache,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageConfidence {
    Authoritative,
    Estimated,
    PresenceOnly,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistryEntry {
    pub codename: String,
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub provider: Option<String>,
    pub started_at: String,
    pub exited_at: Option<String>,
    pub status: String,
    #[serde(default)]
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u64,
    pub label: String,
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub state: AgentState,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub label: String,
    pub focused_pane: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub session_id: u64,
    pub label: String,
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub state: AgentState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_status_report: Option<AgentStatusReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Blocked,
    Done,
    Idle,
    Unknown,
}

impl AgentState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Idle => "idle",
            Self::Unknown => "unknown",
        }
    }
}

pub fn frame(msg: &impl Serialize) -> Vec<u8> {
    let json = serde_json::to_vec(msg).unwrap_or_else(|_| b"{\"type\":\"unknown\"}".to_vec());
    let len = (json.len() as u32).to_be_bytes();
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(&json);
    out
}

#[cfg(test)]
mod tests;
