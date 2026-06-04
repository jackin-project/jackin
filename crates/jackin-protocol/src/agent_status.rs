//! Protocol-level agent status types.
//!
//! These are the wire types exposed over the control socket. The capsule-
//! internal types (`SessionStatus`, `HookAuthority`, screen detection structs)
//! live in `crates/jackin-capsule/src/agent_status/`; only the summary types
//! that must cross the socket boundary are defined here.

use serde::{Deserialize, Serialize};

/// Raw detector state — what detectors and reporters produce before
/// the state machine folds them into an effective status.
///
/// Note: In the capsule implementation, the raw signal is the more expressive
/// 10-variant `AgentRawState` enum in `jackin_capsule::agent_status`. This
/// simpler 4-variant enum is the protocol-level summary used in wire payloads
/// and `AgentStatusReport`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRawState {
    Unknown,
    Working,
    Blocked,
    Idle,
}

impl AgentRawState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Idle    => "idle",
        }
    }
}

/// Source of the current status authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStatusSource {
    /// Reported by a trusted in-container hook/plugin/API bridge.
    Reported { source_id: String },
    /// Derived from visible screen pattern matching.
    VisibleScreen,
    /// Derived from foreground process group identity.
    ForegroundProcess,
    /// Derived from OSC 133/7 shell integration markers.
    ShellIntegration,
    /// Derived from a CSI 6n cursor-position probe response.
    CursorProbe,
    /// Derived from recent PTY output activity (weak signal).
    OutputActivity,
    /// No authority source — state is unknown.
    None,
}

impl Default for AgentStatusSource {
    fn default() -> Self {
        Self::None
    }
}

/// Confidence tier for the current status authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusConfidence {
    /// No signal — state is unknown.
    Unknown,
    /// Derived from process presence or output activity only.
    Weak,
    /// Screen detection matched a clear visible pattern.
    Strong,
    /// Hook authority: sequence-valid, process-consistent, fresh.
    Authoritative,
}

impl Default for AgentStatusConfidence {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Summary status report included in `SessionInfo` and `PaneSnapshot`
/// responses. Carries the raw state, source, confidence, and evidence
/// booleans so host consumers can reason about authority without re-parsing
/// terminal text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatusReport {
    /// Protocol-level raw state from the authority source.
    pub raw_state: AgentRawState,
    /// Source that produced the current authority.
    pub source: AgentStatusSource,
    /// Confidence tier of the current authority.
    pub confidence: AgentStatusConfidence,
    /// Detected agent slug (e.g. `"claude"`, `"codex"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_agent: Option<String>,
    /// Foreground process group ID, if known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreground_pgid: Option<u32>,
    /// Screen detector saw an explicit approval/input prompt.
    #[serde(default)]
    pub visible_blocker: bool,
    /// Screen detector saw an idle prompt box.
    #[serde(default)]
    pub visible_idle: bool,
    /// Screen detector saw active working chrome.
    #[serde(default)]
    pub visible_working: bool,
    /// Child process has exited.
    #[serde(default)]
    pub process_exited: bool,
    /// Hook report was found stale and cleared.
    #[serde(default)]
    pub stale_report: bool,
    /// Monotonic revision counter; incremented on every state change.
    pub revision: u64,
    /// Last revision acknowledged by the operator (seen).
    pub last_seen_revision: u64,
}

impl Default for AgentStatusReport {
    fn default() -> Self {
        Self {
            raw_state: AgentRawState::Unknown,
            source: AgentStatusSource::None,
            confidence: AgentStatusConfidence::Unknown,
            detected_agent: None,
            foreground_pgid: None,
            visible_blocker: false,
            visible_idle: false,
            visible_working: false,
            process_exited: false,
            stale_report: false,
            revision: 0,
            last_seen_revision: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_raw_state_labels() {
        assert_eq!(AgentRawState::Unknown.label(), "unknown");
        assert_eq!(AgentRawState::Working.label(), "working");
        assert_eq!(AgentRawState::Blocked.label(), "blocked");
        assert_eq!(AgentRawState::Idle.label(), "idle");
    }

    #[test]
    fn agent_status_confidence_ordering() {
        assert!(AgentStatusConfidence::Authoritative > AgentStatusConfidence::Strong);
        assert!(AgentStatusConfidence::Strong > AgentStatusConfidence::Weak);
        assert!(AgentStatusConfidence::Weak > AgentStatusConfidence::Unknown);
    }

    #[test]
    fn agent_status_report_default_is_unknown() {
        let r = AgentStatusReport::default();
        assert_eq!(r.raw_state, AgentRawState::Unknown);
        assert_eq!(r.confidence, AgentStatusConfidence::Unknown);
        assert!(!r.visible_blocker);
        assert!(!r.visible_working);
    }

    #[test]
    fn agent_status_report_roundtrips_json() {
        let report = AgentStatusReport {
            raw_state: AgentRawState::Working,
            source: AgentStatusSource::Reported { source_id: "claude-hook".to_string() },
            confidence: AgentStatusConfidence::Authoritative,
            detected_agent: Some("claude".to_string()),
            foreground_pgid: Some(1234),
            visible_working: true,
            revision: 42,
            last_seen_revision: 40,
            ..Default::default()
        };
        let json = serde_json::to_string(&report).unwrap();
        let decoded: AgentStatusReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.raw_state, AgentRawState::Working);
        assert_eq!(decoded.confidence, AgentStatusConfidence::Authoritative);
        assert_eq!(decoded.revision, 42);
    }
}
