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
