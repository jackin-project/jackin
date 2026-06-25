    use super::*;

    fn event(runtime: &str, event: &str) -> RuntimeEvent {
        RuntimeEvent {
            runtime: runtime.to_owned(),
            event: event.to_owned(),
        }
    }

    fn authority_state(effect: GateEffect) -> RawAgentState {
        match effect {
            GateEffect::Authority { state, .. } => state,
            other => panic!("expected authority effect, got {other:?}"),
        }
    }

    fn canonical_turn(runtime: &str) -> &'static [&'static str] {
        match runtime {
            "claude" => &[
                "UserPromptSubmit",
                "PreToolUse",
                "PermissionRequest",
                "PermissionDenied",
                "PostToolUse",
                "Stop",
            ],
            "codex" => &[
                "UserPromptSubmit",
                "PreToolUse",
                "PermissionRequest",
                "permission-resolved",
                "PostToolUse",
                "Stop",
            ],
            "opencode" => &[
                "session.status",
                "tool.execute.before",
                "permission.asked",
                "permission.replied",
                "tool.execute.after",
                "session.idle",
            ],
            "amp" => &[
                "agent.start",
                "tool.call",
                "permission-requested",
                "permission-resolved",
                "tool.result",
                "agent.end",
            ],
            other => panic!("missing recorded turn for {other}"),
        }
    }

    #[test]
    fn recorded_runtime_turn_sequences_map_to_expected_states() {
        for runtime in ["claude", "codex", "opencode", "amp"] {
            let mut state = SourceGateState::default();
            let observed = canonical_turn(runtime)
                .iter()
                .map(|name| authority_state(map_event(&event(runtime, name), &mut state)))
                .collect::<Vec<_>>();

            assert_eq!(
                observed,
                vec![
                    RawAgentState::Working,
                    RawAgentState::Working,
                    RawAgentState::Blocked,
                    RawAgentState::Working,
                    RawAgentState::Working,
                    RawAgentState::Idle,
                ],
                "runtime={runtime}"
            );
        }
    }

    #[test]
    fn permission_stop_stays_blocked_until_resolved() {
        let mut state = SourceGateState::default();
        assert_eq!(
            map_event(&event("claude", "PermissionRequest"), &mut state),
            GateEffect::Authority {
                state: RawAgentState::Blocked,
                pending_permission: true,
                subagents_active: 0,
                notes: Vec::new(),
            }
        );
        assert_eq!(
            map_event(&event("claude", "Stop"), &mut state),
            GateEffect::Authority {
                state: RawAgentState::Blocked,
                pending_permission: true,
                subagents_active: 0,
                notes: vec![EvidenceNote::StopSuppressed],
            }
        );
    }

    #[test]
    fn permission_resolved_unblocks_to_working() {
        let mut state = SourceGateState {
            pending_permission: true,
            subagents_active: 0,
            notes: Vec::new(),
        };
        assert_eq!(
            map_event(&event("claude", "PermissionDenied"), &mut state),
            GateEffect::Authority {
                state: RawAgentState::Working,
                pending_permission: false,
                subagents_active: 0,
                notes: Vec::new(),
            }
        );
    }

    #[test]
    fn stop_with_live_subagent_stays_working() {
        let mut state = SourceGateState::default();
        assert!(matches!(
            map_event(&event("claude", "SubagentStart"), &mut state),
            GateEffect::CounterOnly {
                subagents_active: 1
            }
        ));
        assert_eq!(
            map_event(&event("claude", "Stop"), &mut state),
            GateEffect::Authority {
                state: RawAgentState::Working,
                pending_permission: false,
                subagents_active: 1,
                notes: vec![EvidenceNote::StopSuppressed],
            }
        );
    }

    #[test]
    fn claude_idle_notification_is_not_blocked() {
        let mut state = SourceGateState::default();
        assert_eq!(
            map_event(&event("claude", "Notification:idle_prompt"), &mut state),
            GateEffect::Heartbeat
        );
        assert!(!state.pending_permission);
    }

    #[test]
    fn claude_permission_notification_blocks() {
        let mut state = SourceGateState::default();
        assert!(matches!(
            map_event(
                &event("claude", "Notification:permission_prompt"),
                &mut state
            ),
            GateEffect::Authority {
                state: RawAgentState::Blocked,
                pending_permission: true,
                ..
            }
        ));
    }
