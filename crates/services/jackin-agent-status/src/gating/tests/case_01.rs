// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

        assert!(!state.pending_permission);
        assert_eq!(state.subagents_active, 0);
    }

    let mut state = SourceGateState::default();
    assert_eq!(
        map_event(&event("claude", "Notification:auth_success"), &mut state),
        GateEffect::Heartbeat
    );
    assert!(!state.pending_permission);

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

#[test]
fn enrich_claude_notification_from_payload_subtype() {
    let enriched = enrich_event_name(
        "claude",
        "Notification",
        Some(r#"{"notification_type":"permission_prompt"}"#),
    );
    assert_eq!(enriched, "Notification:permission_prompt");
    let mut state = SourceGateState::default();
    assert_eq!(
        map_event(&event("claude", &enriched), &mut state),
        GateEffect::Authority {
            state: RawAgentState::Blocked,
            pending_permission: true,
            subagents_active: 0,
            notes: Vec::new(),
        }
    );

    assert_eq!(
        enrich_event_name("claude", "Notification", None),
        "Notification"
    );
    assert_eq!(
        map_event(
            &event("claude", "Notification"),
            &mut SourceGateState::default()
        ),
        GateEffect::Ignore
    );
}
