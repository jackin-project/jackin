//! Control channel: length-prefixed JSON request / response messages.
//!
//! Used by the host CLI for one-shot queries — `status`, `snapshot`,
//! and future `session.create` / `session.kill` / `session.title` /
//! `events`. The host opens a Unix socket connection, writes one
//! framed JSON request, reads one framed JSON response, and
//! disconnects.
use serde::{Deserialize, Serialize};

use crate::agent_status::AgentStatusReport;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Request the current session inventory.
    Status,
    /// Request the tab/pane tree snapshot.
    Snapshot,
    /// Request the agent registry (codenames, agent types, providers, timestamps).
    Agents,
    /// Forward a runtime hook/plugin event for a session from an in-container
    /// reporter. The daemon maps and gates it (events, never states); the
    /// reporter only forwards. Acked immediately so the reporter never blocks
    /// an agent hook.
    ReportRuntimeEvent {
        session_id: u64,
        /// Unique per session+runtime, e.g. `hook-<runtime>-<session>`.
        source_id: String,
        /// Agent runtime slug (`claude`, `codex`, `opencode`, `amp`, …).
        runtime: String,
        /// Vendor event name (`Stop`, `permission.asked`, …) or a canonical name.
        event: String,
        /// Optional raw JSON payload from the hook's stdin (unused for now).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        payload: Option<String>,
    },
    /// Ask the daemon to snapshot a session's live grid + evidence bundle into a
    /// new capture fixture directory (a contributor diagnostic: turn a live
    /// mis-detection into a regression fixture). The daemon owns the grid, so it
    /// writes the files; the client only triggers and is Acked.
    StatusCapture { session_id: u64 },
    /// Request the usage/quota snapshot for the currently focused pane.
    UsageFocused,
    /// Ask the daemon to refresh focused usage/quota data, then return the
    /// current cached snapshot immediately.
    UsageRefreshFocused,
    /// Return every account/quota snapshot currently known to the daemon cache.
    UsageAccountList,
    /// `jackin-exec <command> [args…]` — run a command with operator-approved
    /// on-demand credentials injected at exec time. The daemon shows the
    /// credential picker, resolves selections via the host socket, runs the
    /// command, and replies with `ExecResult` or `ExecDenied`.
    ExecCommand { command: String, args: Vec<String> },
    /// Request the per-session token-spend summary for one session, read from
    /// the daemon's token monitor (provider JSONL/SQLite totals).
    TokenUsage { session_id: u64 },
    /// Forward-compat sink for variants added by a newer peer.
    #[serde(other)]
    Unknown,
}

impl ServerMsg {
    /// Variant name for diagnostics. Canonical home for the variant→label map so
    /// consumers across crates don't each re-spell it.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionList { .. } => "SessionList",
            Self::Snapshot { .. } => "Snapshot",
            Self::AgentRegistry { .. } => "AgentRegistry",
            Self::UsageFocused { .. } => "UsageFocused",
            Self::UsageAccounts { .. } => "UsageAccounts",
            Self::ExecResult { .. } => "ExecResult",
            Self::ExecDenied { .. } => "ExecDenied",
            Self::TokenUsage { .. } => "TokenUsage",
            Self::Ack => "Ack",
            Self::Unknown => "Unknown",
        }
    }
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
    /// Acknowledgement for a fire-and-forget request (e.g. `ReportRuntimeEvent`).
    Ack,
    /// Usage/quota data for the focused pane.
    UsageFocused { usage: Box<FocusedUsageView> },
    /// Account/quota snapshots known to the daemon cache.
    UsageAccounts {
        accounts: Vec<AccountUsageSnapshotView>,
    },
    /// Result of a `jackin-exec` invocation: the child's exit code and its
    /// (capped, secret-redacted) stdout/stderr. `redacted_count` reports how
    /// many secret patterns were scrubbed from the output.
    ExecResult {
        exit_code: i32,
        stdout: String,
        stderr: String,
        redacted_count: u32,
    },
    /// A `jackin-exec` invocation the daemon refused to run (operator cancelled
    /// the picker, the host resolver was unavailable, or `op read` failed). No
    /// command was executed.
    ExecDenied { reason: String },
    /// Per-session token-spend summary; `None` when the session is unknown to
    /// the token monitor (never registered, or already exited).
    TokenUsage { summary: Option<TokenUsageSummary> },
    /// Forward-compat sink for variants added by a newer peer.
    #[serde(other)]
    Unknown,
}

/// Per-session token-spend totals reported by the in-container token monitor.
/// Mirrors `token_monitor::TokenTotals::to_summary` on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenUsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Provider-supplied cost when the source reports it directly.
    pub cost_usd: Option<f64>,
    /// Most recently used model in the session.
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
    /// Status-bar headline. Carries the percentage windows and, when the
    /// focused account has a monetary spend window, the spend joined in:
    /// `Session 89% · Weekly 73% · SGD 78 of 260`.
    pub status_bar_label: String,
    pub tabs: Vec<UsageProviderTab>,
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

    /// The agent has started but its usage data is not yet known — an honest
    /// "loading" state, distinct from `unavailable` (genuinely no data) and
    /// from a hidden segment (no agent at all). Carries no fabricated numbers.
    #[must_use]
    pub fn refreshing(provider: Option<&str>, now_epoch: i64) -> Self {
        let mut view = Self::unavailable("refreshing", now_epoch);
        view.focused_provider = provider.map(str::to_owned);
        view.account.provider_label = provider.unwrap_or("Usage").to_owned();
        view.account.account_label = String::new();
        view.status_bar_label = "refreshing".to_owned();
        view.updated_label = "Refreshing".to_owned();
        view
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FocusedAccountHeader {
    pub provider_label: String,
    /// Account identity: the real account email when the provider exposes one,
    /// otherwise empty (no fabricated identity). Auth method/source belongs in
    /// `credential_origin`, not here.
    pub account_label: String,
    /// Account username/handle, when distinct from the email.
    #[serde(default)]
    pub username: Option<String>,
    pub plan_label: Option<String>,
    /// Where the credential came from (the auth source), never the secret:
    /// e.g. `OAuth · keychain`, `API token · env ZAI_API_KEY`,
    /// `API key · amp secrets.json`. `None` until populated.
    #[serde(default)]
    pub credential_origin: Option<String>,
}

/// Which slot of the canonical `Session N% · Weekly N%` status-bar headline a
/// quota window fills. Set by the provider snapshot that builds the bucket (it
/// knows the window's role), so the status bar reads this semantic tag instead
/// of substring-matching free-text labels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StatusSlot {
    Session,
    Weekly,
    /// Monetary spend against a cap (Claude `extra_usage`/`spend`, Codex credits).
    /// Rendered in the status bar as money (`$53/$300`) from the bucket's
    /// [`QuotaBucketView::used_money`]/[`limit_money`], not as a `%`.
    Spend,
}

/// A monetary amount with explicit scale, so a value sourced in minor units
/// (cents) is never mis-rendered as major units. The Claude/OpenAI usage APIs
/// report money as `{ amount_minor, currency, exponent }`; carrying that shape
/// end-to-end removes the whole class of "100×-too-large" formatting bugs that
/// a bare `f64 + currency` representation invited.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Money {
    /// Amount in the currency's minor unit (e.g. cents). `5331` with
    /// `exponent = 2` is `53.31`.
    pub amount_minor: i64,
    /// ISO-4217 code (`"USD"`, `"SGD"`) or a non-standard credits label.
    pub currency: String,
    /// Decimal places: minor = major × 10^exponent. Almost always `2`.
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

    /// Major-unit value (e.g. `53.31`). Pure scale conversion; no rounding loss
    /// for the `<= 2` exponents these APIs use.
    #[must_use]
    pub fn major(&self) -> f64 {
        self.amount_minor as f64 / 10f64.powi(i32::from(self.exponent))
    }

    /// Compact label for the width-constrained status bar: no minor units
    /// (`$53`, `SGD 78`), rounded to the nearest major unit. The full-precision
    /// form is the [`Display`](std::fmt::Display) impl.
    #[must_use]
    pub fn format_compact(&self) -> String {
        self.format_with_precision(0)
    }

    /// Bare major-unit amount, rounded, with no currency (`260`). Used for the
    /// limit side of a `<used> of <limit>` headline where the currency is
    /// already shown on the used side.
    #[must_use]
    pub fn major_amount(&self) -> i64 {
        self.major().round() as i64
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

/// Full-precision label. `USD` renders with a leading `$` (`$53.31`); any other
/// ISO-4217 three-letter code renders as `CODE 53.31` (e.g. `SGD 78.49`); a
/// non-standard label (credits) renders as `53.31 credits`.
impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.format_with_precision(usize::from(self.exponent)))
    }
}

/// Severity of a quota/spend window, mirrored from the API so the meter and
/// status bar can color-grade approaching limits instead of inferring from a
/// raw percentage.
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
    /// Raw reset timestamp (epoch seconds) behind `reset_label`. Kept so the
    /// CLI report (`usage accounts`) can emit `resets_at` instead of dropping
    /// it — the formatted `reset_label` alone cannot be reversed (RC2).
    #[serde(default)]
    pub resets_at: Option<i64>,
    /// Which status-bar headline slot this window fills, if any.
    #[serde(default)]
    pub status_slot: Option<StatusSlot>,
    pub pace_label: Option<String>,
    pub status: UsageSnapshotStatus,
    /// Structured spent amount behind `used_label`, when the window is monetary
    /// (the `Spend` slot). Carried as [`Money`] so the status bar can format
    /// `$53/$300` at the edge instead of trusting a preformatted string — this
    /// is what keeps minor-unit values from rendering 100× too large.
    #[serde(default)]
    pub used_money: Option<Money>,
    /// Structured cap behind `limit_label`, when monetary. `None` = uncapped.
    #[serde(default)]
    pub limit_money: Option<Money>,
    /// API-reported severity for color-grading the meter / status chip.
    #[serde(default)]
    pub severity: UsageSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UsageProviderTab {
    pub label: String,
    pub status_label: String,
    pub account_label: String,
    pub plan_label: Option<String>,
    /// Freshness + source tag for the Overview row, e.g. "fresh · provider"
    /// or "stale · local estimate". `None` until the daemon enriches the tab.
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
    /// Full evidence-arbitration status report. `None` until the capsule
    /// populates it from `SessionStatus::report` (Phase 3/10 wiring); the host
    /// console renders `state` until then.
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
    /// No reliable evidence about the agent's state. Safer than guessing.
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
