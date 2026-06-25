//! Seen-flag management for agent status acknowledgement.
//!
//! The `seen` flag is embedded in `SessionStatus` in the parent module;
//! this module provides the public functions for acknowledging sessions
//! and marking panes as focused.

use crate::agent_status::SessionStatus;
use crate::protocol::AgentState;

/// Mark a session as seen by the operator.
/// Transitions `Done` → `Idle`; returns `Some(Idle)` when it changed.
pub fn acknowledge_session(status: &mut SessionStatus) -> Option<AgentState> {
    status.acknowledge()
}

/// Mark a pane as focused — equivalent to `acknowledge_session` for the
/// focused pane. Called by `refresh_session_statuses` each tick for the
/// active pane.
pub fn mark_pane_focused(status: &mut SessionStatus) -> Option<AgentState> {
    acknowledge_session(status)
}

#[cfg(test)]
mod tests;
