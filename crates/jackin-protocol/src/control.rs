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
    /// Runtime reporter sends raw agent state for one session.
    ReportAgentState {
        session_id: u64,
        source_id: String,
        agent_label: String,
        /// Raw state: "working", "blocked", "idle", "unknown"
        raw_state: String,
        /// Monotonic sequence number (nanosecond timestamp as u64).
        seq: u64,
        /// Nanoseconds since UNIX epoch.
        ts_ns: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Runtime reporter heartbeat — confirms source is still alive.
    HeartbeatAgentAuthority {
        session_id: u64,
        source_id: String,
        seq: u64,
    },
    /// Runtime reporter releases its authority (exits or goes stale).
    ClearAgentAuthority { session_id: u64, source_id: String },
    /// Runtime bridge reports descendant/subagent lifecycle.
    ReportChildAgentState {
        parent_session_id: u64,
        child_session_id: u64,
        raw_state: String,
        seq: u64,
    },
    /// Subscribe to agent state change events. After this message the
    /// connection becomes a persistent streaming channel.
    EventsSubscribe {
        #[serde(skip_serializing_if = "Option::is_none")]
        subscriber_id: Option<String>,
    },
    /// Block until a session reaches one of the target statuses.
    WaitSessionStatus {
        session_id: u64,
        target_statuses: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timeout_ms: Option<u64>,
    },
    /// Read visible pane text for debugging.
    SessionReadVisible {
        session_id: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        rows: Option<u16>,
    },
    /// One-shot query for current token totals for a session.
    TokenGetSession { session_id: u64 },
    /// Query the model catalog for available models.
    TokenGetModels {
        #[serde(skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
    },
    /// Request the agent registry (codenames, agent types, providers, timestamps).
    Agents,
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
    /// Pushed to subscribed clients on every effective-status change.
    AgentStateChanged {
        session_id: u64,
        /// Raw detector state: "working", "blocked", "idle", "unknown"
        #[serde(skip_serializing_if = "Option::is_none")]
        raw_state: Option<String>,
        /// Effective status: "working", "blocked", "done", "idle", "unknown"
        effective: String,
        seen: bool,
        /// Authority source description
        source: String,
        /// Confidence tier: "authoritative", "strong", "weak", "unknown"
        #[serde(skip_serializing_if = "Option::is_none")]
        confidence: Option<String>,
        /// Detected agent slug, if identified
        #[serde(skip_serializing_if = "Option::is_none")]
        detected_agent: Option<String>,
        /// Foreground process group ID
        #[serde(skip_serializing_if = "Option::is_none")]
        foreground_pgid: Option<u32>,
        /// Screen detector saw an explicit approval/input prompt
        #[serde(default)]
        visible_blocker: bool,
        /// Screen detector saw an idle prompt
        #[serde(default)]
        visible_idle: bool,
        /// Screen detector saw active working chrome
        #[serde(default)]
        visible_working: bool,
        /// Child process has exited
        #[serde(default)]
        process_exited: bool,
        /// Hook report was found stale and cleared
        #[serde(default)]
        stale_report: bool,
        /// Monotonic sequence number (nanosecond timestamp)
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
        /// Nanoseconds since UNIX epoch when the event was emitted
        #[serde(skip_serializing_if = "Option::is_none")]
        ts_ns: Option<u64>,
        revision: u64,
        /// Last revision seen by the operator
        #[serde(skip_serializing_if = "Option::is_none")]
        last_seen_revision: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// A new session has been created.
    SessionSpawned {
        session_id: u64,
        agent: Option<String>,
        label: String,
    },
    /// A session has exited.
    SessionExited { session_id: u64 },
    /// Token totals for a session have been updated.
    TokenUsageChanged {
        session_id: u64,
        agent: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
        ts_ns: u64,
    },
    /// Workspace-level roll-up status changed.
    WorkspaceStatusChanged {
        effective: String,
        session_count: u32,
        blocked_count: u32,
        done_count: u32,
        working_count: u32,
        ts_ns: u64,
    },
    /// Response to TokenGetSession.
    TokenSessionResult {
        session_id: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        token_usage: Option<TokenUsageSummary>,
    },
    /// Response to TokenGetModels.
    TokenModelsResult {
        provider: String,
        models: Vec<String>,
    },
    /// Response to WaitSessionStatus — the current state at the time the wait resolved.
    SessionStatusResult {
        session_id: u64,
        effective: String,
        revision: u64,
        /// "satisfied", "timeout", "not_found"
        outcome: String,
    },
    /// Response to SessionReadVisible.
    SessionVisibleText { session_id: u64, lines: Vec<String> },
    /// Welcome frame sent to every connecting client.
    Welcome { jackin_protocol_version: String },
    /// Error response.
    Error { code: String, message: String },
    /// Agent registry: every tab ever opened in this container lifetime.
    AgentRegistry { records: Vec<AgentRegistryEntry> },
    /// Forward-compat sink for variants added by a newer peer.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_usage: Option<TokenUsageSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_status_report: Option<crate::agent_status::AgentStatusReport>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_status_report: Option<crate::agent_status::AgentStatusReport>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Blocked,
    Done,
    Idle,
    /// State not yet determined. Safer default than `Blocked` when no
    /// reliable signal is available. Phase 1 arbitration will replace this
    /// with a real detection result.
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
