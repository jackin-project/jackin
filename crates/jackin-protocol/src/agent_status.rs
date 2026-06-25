//! Wire types for the agent runtime status authority.
//!
//! These are the shared, serializable status types the capsule produces and the
//! host console consumes. The detection state machine itself lives in the
//! capsule (`jackin-capsule::agent_status`); only the values that cross the
//! control socket belong here. See the roadmap item
//! `reference/roadmap/agent-runtime-status` for the design.

use serde::{Deserialize, Serialize};

/// Raw detector state, before `done` is derived from raw `idle` + unseen.
///
/// This is the four-state vocabulary every evidence source maps onto. The
/// public `Done` state is derived later (raw `Idle` on an unseen pane) and is
/// never a raw state. `Unknown` is the safe default when no reliable evidence
/// exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRawState {
    Unknown,
    Working,
    Blocked,
    Idle,
}

/// Confidence of the evidence that produced a raw state.
///
/// Ordered weakest to strongest; arbitration and the debounce policy compare
/// confidences (`>=`, `<`), so the variant order is load-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusConfidence {
    Unknown,
    Weak,
    Strong,
    Authoritative,
}

/// What evidence channel won the arbitration that produced the reported state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusSource {
    /// No attributable source (no evidence, or an exit/unknown outcome).
    None,
    /// A screen rule-pack match over the live grid.
    VisibleScreen,
    /// OSC 133 shell-integration markers (shell panes).
    ShellIntegration,
    /// `/proc` foreground-process physics.
    ForegroundProcess,
    /// A runtime hook/plugin or cooperative reporter, identified by source id.
    Reported { source_id: String },
}

/// A point-in-time status report for one session, sent on the control socket
/// and rendered by the host console. Every field is computed from arbitration
/// inputs, never from the output state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStatusReport {
    pub raw_state: AgentRawState,
    pub source: AgentStatusSource,
    pub confidence: AgentStatusConfidence,
    pub detected_agent: Option<String>,
    pub foreground_pgid: Option<u32>,
    pub visible_blocker: bool,
    pub visible_idle: bool,
    pub visible_working: bool,
    pub process_exited: bool,
    pub foreground_returned_to_shell: bool,
    pub stale_report: bool,
    pub subagents_active: u32,
    pub revision: u64,
}
