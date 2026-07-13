// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    /// `Unknown` variant.
    Unknown,
    /// `Working` variant.
    Working,
    /// `Blocked` variant.
    Blocked,
    /// `Idle` variant.
    Idle,
}

/// Confidence of the evidence that produced a raw state.
///
/// Ordered weakest to strongest; arbitration and the debounce policy compare
/// confidences (`>=`, `<`), so the variant order is load-bearing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatusConfidence {
    /// `Unknown` variant.
    Unknown,
    /// `Weak` variant.
    Weak,
    /// `Strong` variant.
    Strong,
    /// `Authoritative` variant.
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
    Reported {
        /// Identifier of the cooperative reporter / hook source.
        source_id: String,
    },
}

/// A point-in-time status report for one session, sent on the control socket
/// and rendered by the host console. Every field is computed from arbitration
/// inputs, never from the output state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "Six orthogonal arbitrated-state wire flags (visible_blocker, \
              visible_idle, visible_working, process_exited, \
              foreground_returned_to_shell, stale_report) — each is an independent \
              observable serialized to the control socket and consumed individually \
              by the host console. Named-field reads match the per-signal \
              wire-payload idiom this struct parallels."
)]
pub struct AgentStatusReport {
    /// `raw_state` field.
    pub raw_state: AgentRawState,
    /// `source` field.
    pub source: AgentStatusSource,
    /// `confidence` field.
    pub confidence: AgentStatusConfidence,
    /// `detected_agent` field.
    pub detected_agent: Option<String>,
    /// `foreground_pgid` field.
    pub foreground_pgid: Option<u32>,
    /// `visible_blocker` field.
    pub visible_blocker: bool,
    /// `visible_idle` field.
    pub visible_idle: bool,
    /// `visible_working` field.
    pub visible_working: bool,
    /// `process_exited` field.
    pub process_exited: bool,
    /// `foreground_returned_to_shell` field.
    pub foreground_returned_to_shell: bool,
    /// `stale_report` field.
    pub stale_report: bool,
    /// `subagents_active` field.
    pub subagents_active: u32,
    /// `revision` field.
    pub revision: u64,
}

#[cfg(test)]
mod tests;
