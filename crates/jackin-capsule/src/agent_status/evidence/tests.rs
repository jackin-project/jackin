    use super::*;

    #[test]
    fn clearing_agent_osc_signals_preserves_shell_markers() {
        let now = Instant::now();
        let mut evidence = OscEvidence {
            title: Some("Codex working".to_owned()),
            title_changed_at: Some(now),
            notify_edge_at: Some(now),
            progress_active: true,
            progress_cleared_at: Some(now),
            bel_at: Some(now),
            bel_count: 2,
            shell_state: Some(RawAgentState::Idle),
            shell_mark_at: Some(now),
        };

        evidence.clear_agent_signals();

        assert_eq!(evidence.title, None);
        assert_eq!(evidence.title_changed_at, None);
        assert_eq!(evidence.notify_edge_at, None);
        assert!(!evidence.progress_active);
        assert_eq!(evidence.progress_cleared_at, None);
        assert_eq!(evidence.bel_at, None);
        assert_eq!(evidence.bel_count, 0);
        assert_eq!(evidence.shell_state, Some(RawAgentState::Idle));
        assert_eq!(evidence.shell_mark_at, Some(now));
    }
