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
mod tests {
    use super::*;
    use crate::agent_status::SessionStatus;
    use crate::agent_status::evidence::{EvidenceSummary, RawAgentState};
    use jackin_protocol::agent_status::AgentStatusConfidence;

    fn publish_done(status: &mut SessionStatus) {
        status.publish_raw(
            RawAgentState::Working,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
        status.publish_raw(
            RawAgentState::Idle,
            AgentStatusConfidence::Strong,
            EvidenceSummary::default(),
        );
    }

    #[test]
    fn acknowledge_session_transitions_done_to_idle() {
        let mut status = SessionStatus::new();
        publish_done(&mut status);
        assert_eq!(status.effective, AgentState::Done);
        let result = acknowledge_session(&mut status);
        assert_eq!(result, Some(AgentState::Idle));
        assert_eq!(status.effective, AgentState::Idle);
    }

    #[test]
    fn mark_pane_focused_clears_done() {
        let mut status = SessionStatus::new();
        publish_done(&mut status);
        let result = mark_pane_focused(&mut status);
        assert_eq!(result, Some(AgentState::Idle));
    }
}
