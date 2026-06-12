//! Control channel: length-prefixed JSON request / response messages.
//!
//! Used by the host CLI for one-shot queries — `status`, `snapshot`,
//! and future `session.create` / `session.kill` / `session.title` /
//! `events`. The host opens a Unix socket connection, writes one
//! framed JSON request, reads one framed JSON response, and
//! disconnects.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Request the current session inventory.
    Status,
    /// Request the tab/pane tree snapshot.
    Snapshot,
    /// Request the agent registry (codenames, agent types, providers, timestamps).
    Agents,
    /// Request the usage/quota snapshot for the currently focused pane.
    UsageFocused,
    /// Ask the daemon to refresh focused usage/quota data, then return the
    /// current cached snapshot immediately.
    UsageRefreshFocused,
    /// Return every account/quota snapshot currently known to the daemon cache.
    UsageAccountList,
    /// Return local usage attribution for a workspace from cached samples.
    UsageWorkspace {
        workspace: Option<String>,
        window_seconds: Option<i64>,
    },
    /// Return local usage attribution for one Capsule session from cached samples.
    UsageSession {
        session_id: i64,
        window_seconds: Option<i64>,
    },
    /// Forward-compat sink for variants added by a newer peer.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Current session inventory.
    SessionList { sessions: Vec<SessionInfo> },
    /// Tab/pane tree snapshot. `tabs` is in render order;
    /// `active_tab` indexes into it. Each `TabSnapshot::panes` lists
    /// the pane leaves of that tab in `PaneTree` in-order traversal
    /// order; `TabSnapshot::focused_pane` carries the session id of
    /// the focused leaf (matches a `PaneSnapshot::session_id`).
    Snapshot {
        tabs: Vec<TabSnapshot>,
        active_tab: u32,
    },
    /// Agent registry: every tab ever opened in this container lifetime.
    AgentRegistry { records: Vec<AgentRegistryEntry> },
    /// Usage/quota data for the focused pane.
    UsageFocused { usage: Box<FocusedUsageView> },
    /// Account/quota snapshots known to the daemon cache.
    UsageAccounts {
        accounts: Vec<AccountUsageSnapshotView>,
    },
    /// Local token/cost summary from cached usage samples.
    UsageSummary { summary: UsageSummaryView },
    /// Forward-compat sink for variants added by a newer peer.
    #[serde(other)]
    Unknown,
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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageSummaryView {
    pub workspace: Option<String>,
    pub session_id: Option<i64>,
    pub window_seconds: Option<i64>,
    pub sample_count: u64,
    pub token_input: u64,
    pub token_output: u64,
    pub token_cache_read: u64,
    pub token_cache_write: u64,
    pub cost_usd_micros: u64,
    pub exact_cost_sample_count: u64,
    pub estimated_cost_sample_count: u64,
    pub unpriced_sample_count: u64,
    pub first_occurred_at: Option<i64>,
    pub last_occurred_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedUsageView {
    pub focused_agent: Option<String>,
    pub focused_provider: Option<String>,
    pub account: FocusedAccountHeader,
    pub buckets: Vec<QuotaBucketView>,
    pub workspace_spend: WorkspaceSpendView,
    pub status: UsageSnapshotStatus,
    pub source: UsageSource,
    pub confidence: UsageConfidence,
    pub fetched_at_epoch: i64,
    pub updated_label: String,
    pub status_bar_label: String,
    pub provider_status: Option<ProviderStatusView>,
    pub tabs: Vec<UsageProviderTab>,
    pub instance: Option<InstanceUsageView>,
    pub last_error: Option<String>,
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
                plan_label: None,
            },
            buckets: Vec::new(),
            workspace_spend: WorkspaceSpendView::default(),
            status: UsageSnapshotStatus::Unavailable,
            source: UsageSource::None,
            confidence: UsageConfidence::None,
            fetched_at_epoch: now_epoch,
            updated_label: "Unavailable".to_owned(),
            status_bar_label: "usage unavailable".to_owned(),
            provider_status: None,
            tabs: Vec::new(),
            instance: None,
            last_error: Some(reason),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceUsageView {
    pub instance_label: String,
    pub started_at_epoch: Option<i64>,
    pub age_label: String,
    pub workspace: String,
    pub total: UsageSummaryView,
    pub agent_rows: Vec<InstanceAgentUsageRow>,
    pub provider_rows: Vec<InstanceProviderUsageRow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceAgentUsageRow {
    pub codename: String,
    pub session_id: u64,
    pub agent_label: String,
    pub provider_label: String,
    pub account_label: String,
    pub lifecycle_label: String,
    pub started_at_epoch: Option<i64>,
    pub exited_at_epoch: Option<i64>,
    pub spend: UsageSummaryView,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstanceProviderUsageRow {
    pub provider_label: String,
    pub account_label: String,
    pub spend: UsageSummaryView,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedAccountHeader {
    pub provider_label: String,
    pub account_label: String,
    pub plan_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuotaBucketView {
    pub label: String,
    pub used_label: Option<String>,
    pub limit_label: Option<String>,
    pub remaining_percent: Option<u8>,
    pub reset_label: Option<String>,
    pub pace_label: Option<String>,
    pub status: UsageSnapshotStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspaceSpendView {
    pub today_cost_label: Option<String>,
    pub thirty_day_cost_label: Option<String>,
    pub thirty_day_tokens_label: Option<String>,
    pub latest_tokens_label: Option<String>,
    pub top_model: Option<String>,
    pub history: Vec<u64>,
    pub provenance_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderStatusView {
    pub label: String,
    pub detail: String,
    pub updated_label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageProviderTab {
    pub label: String,
    pub status_label: String,
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

/// One entry in the agent registry, representing a tab that was (or is) open.
///
/// Active agents have `exited_at == None`. Exited agents retain their record
/// permanently so `jackin-capsule agents` can show session history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRegistryEntry {
    /// Human-readable codename assigned to the tab (e.g. `"badger"`).
    pub codename: String,
    /// Agent slug (`"claude"`, `"codex"`, …), or `None` for shell sessions.
    pub agent: Option<String>,
    /// Provider label (e.g. `"anthropic"`, `"openai"`), or `None` when no
    /// provider was selected. Default for `claude` is `"anthropic"`;
    /// for `codex` is `"openai"`. Other runtimes have no inferred default.
    pub provider: Option<String>,
    /// ISO 8601 UTC timestamp when the tab was opened.
    pub started_at: String,
    /// ISO 8601 UTC timestamp when the tab was closed, or `None` if still active.
    pub exited_at: Option<String>,
    /// `"active"` or `"exited"`.
    pub status: String,
    /// `true` when this entry represents the calling process's own tab.
    /// Set by `run_agents` by comparing `JACKIN_AGENT_CODENAME` against the codename.
    #[serde(default)]
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u64,
    pub label: String,
    pub agent: Option<String>,
    pub state: AgentState,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub label: String,
    /// `session_id` of the focused leaf in this tab. Always matches
    /// one of the `panes[*].session_id` entries.
    pub focused_pane: u64,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub session_id: u64,
    /// Session label (agent slug or "Shell").
    pub label: String,
    /// `None` for shell sessions; the agent slug otherwise.
    pub agent: Option<String>,
    pub state: AgentState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Blocked,
    Done,
    Idle,
}

impl AgentState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Idle => "idle",
        }
    }
}

/// Encode `msg` as a 4-byte big-endian length prefix + UTF-8 JSON body.
///
/// `to_vec` cannot actually fail for `ClientMsg` or `ServerMsg` — their
/// derived `Serialize` impls only emit JSON-representable variants. If a
/// future generic caller breaks that invariant, encode `Unknown` instead of
/// panicking or shipping a 4-byte length=0 frame the peer interprets as an
/// empty payload.
///
/// `ServerMsg::Unknown` IS a legitimate reply (socket.rs returns it as
/// the response to an unknown `ClientMsg` so the peer's `read_exact`
/// returns immediately instead of hanging until `SOCKET_TIMEOUT`), so
/// the encode side intentionally serializes it as `{"type":"unknown"}`.
/// Peers re-decode it as `Unknown` and the host CLI surfaces the
/// mismatch as an operator-facing error.
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
