use super::*;

fn event<'a>(runtime: &'a str, event: &'a str) -> RuntimeEvent<'a> {
    RuntimeEvent { runtime, event }
}

fn authority_state(effect: GateEffect) -> RawAgentState {
    match effect {
        GateEffect::Authority { state, .. } => state,
        other => panic!("expected authority effect, got {other:?}"),
    }
}

// Complete-grade (OpenCode) and Amp vendor turns still author state. Claude
// and Codex are identity-only (Decision 0a) — covered separately below.
fn canonical_turn(runtime: &str) -> &'static [&'static str] {
    match runtime {
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
    for runtime in ["opencode", "amp"] {
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
fn claude_and_codex_hooks_are_identity_only() {
    // Decision 0a: every Claude/Codex lifecycle hook event refreshes
    // freshness only (Heartbeat) and never authors state. Only Claude's
    // SessionEnd is an identity edge (Clear). This is what prevents the
    // post-Stop SubagentStop/recap from reviving an idle pane.
    for runtime in ["claude", "codex"] {
        let mut state = SourceGateState::default();
        for name in [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PermissionRequest",
            "PermissionDenied",
            "PostToolUse",
            "SubagentStart",
            "SubagentStop",
            "Stop",
        ] {
            assert_eq!(
                map_event(&event(runtime, name), &mut state),
                GateEffect::Heartbeat,
                "runtime={runtime} event={name} must be identity-only"
            );
        }
        // State never moved off the default; no spurious pending permission.
        assert!(!state.pending_permission);
        assert_eq!(state.subagents_active, 0);
    }
    // Unknown notification types are still freshness-only.
    let mut state = SourceGateState::default();
    assert_eq!(
        map_event(&event("claude", "Notification:auth_success"), &mut state),
        GateEffect::Heartbeat
    );
    assert!(!state.pending_permission);
    // The one carry-through identity edge: Claude SessionEnd clears.
    assert_eq!(
        map_event(
            &event("claude", "SessionEnd"),
            &mut SourceGateState::default()
        ),
        GateEffect::Clear
    );
}

#[test]
fn claude_notification_wait_states_author_partial_authority() {
    let mut state = SourceGateState::default();
    assert_eq!(
        map_event(
            &event("claude", "Notification:permission_prompt"),
            &mut state
        ),
        GateEffect::Authority {
            state: RawAgentState::Blocked,
            pending_permission: true,
            subagents_active: 0,
            notes: Vec::new(),
        }
    );

    assert_eq!(
        map_event(&event("claude", "Notification:idle_prompt"), &mut state),
        GateEffect::Authority {
            state: RawAgentState::Blocked,
            pending_permission: true,
            subagents_active: 0,
            notes: vec![EvidenceNote::StopSuppressed],
        },
        "idle notification must not clear an unresolved permission prompt"
    );

    state.pending_permission = false;
    assert_eq!(
        map_event(&event("claude", "Notification:idle_prompt"), &mut state),
        GateEffect::Authority {
            state: RawAgentState::Idle,
            pending_permission: false,
            subagents_active: 0,
            notes: Vec::new(),
        }
    );

    assert_eq!(
        map_event(
            &event("claude", "Notification:elicitation_dialog"),
            &mut state
        ),
        GateEffect::Authority {
            state: RawAgentState::Blocked,
            pending_permission: true,
            subagents_active: 0,
            notes: Vec::new(),
        }
    );
}

#[cfg(feature = "codex-app-server-authority")]
#[test]
fn codex_app_server_turn_events_author_flagged_complete_authority() {
    let mut state = SourceGateState::default();
    assert_eq!(
        map_event(&event("codex-app-server", "turn/started"), &mut state),
        GateEffect::Authority {
            state: RawAgentState::Working,
            pending_permission: false,
            subagents_active: 0,
            notes: Vec::new(),
        }
    );
    assert_eq!(
        map_event(&event("codex-app-server", "turn/completed"), &mut state),
        GateEffect::Authority {
            state: RawAgentState::Idle,
            pending_permission: false,
            subagents_active: 0,
            notes: Vec::new(),
        }
    );
}

#[test]
fn permission_stop_stays_blocked_until_resolved() {
    // Canonical gating logic (runtime-agnostic): a turn-complete while a
    // permission is pending stays Blocked, suppressed.
    let mut state = SourceGateState::default();
    assert_eq!(
        map_event(&event("opencode", "permission-requested"), &mut state),
        GateEffect::Authority {
            state: RawAgentState::Blocked,
            pending_permission: true,
            subagents_active: 0,
            notes: Vec::new(),
        }
    );
    assert_eq!(
        map_event(&event("opencode", "stop"), &mut state),
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
    };
    assert_eq!(
        map_event(&event("opencode", "permission.replied"), &mut state),
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
        map_event(&event("opencode", "subagent-start"), &mut state),
        GateEffect::CounterOnly {
            subagents_active: 1
        }
    ));
    assert_eq!(
        map_event(&event("opencode", "session.idle"), &mut state),
        GateEffect::Authority {
            state: RawAgentState::Working,
            pending_permission: false,
            subagents_active: 1,
            notes: vec![EvidenceNote::StopSuppressed],
        }
    );
}
