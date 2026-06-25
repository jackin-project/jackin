use super::*;

#[test]
fn clearing_agent_osc_signals_preserves_shell_state() {
    let now = Instant::now();
    let mut evidence = OscEvidence {
        title: Some("Codex working".to_owned()),
        progress_active: true,
        progress_cleared_at: Some(now),
        progress_raw: Some("4;1".to_owned()),
        shell_state: Some(RawAgentState::Idle),
    };

    evidence.clear_agent_signals();

    assert_eq!(evidence.title, None);
    assert!(!evidence.progress_active);
    assert_eq!(evidence.progress_cleared_at, None);
    assert_eq!(evidence.progress_raw, None);
    // Shell integration state survives the agent-signal clear.
    assert_eq!(evidence.shell_state, Some(RawAgentState::Idle));
}
