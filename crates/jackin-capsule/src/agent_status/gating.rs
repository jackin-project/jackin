// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::agent_status::evidence::{EvidenceNote, RawAgentState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeEvent<'a> {
    pub runtime: &'a str,
    pub event: &'a str,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceGateState {
    pub pending_permission: bool,
    pub subagents_active: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateEffect {
    Authority {
        state: RawAgentState,
        pending_permission: bool,
        subagents_active: u32,
        notes: Vec<EvidenceNote>,
    },
    Heartbeat,
    Clear,
    CounterOnly {
        subagents_active: u32,
    },
    Ignore,
}

pub fn map_event(event: &RuntimeEvent<'_>, state: &mut SourceGateState) -> GateEffect {
    let Some(canonical) = canonical_event(event.runtime, event.event) else {
        return GateEffect::Ignore;
    };
    match canonical {
        "prompt-submitted" | "tool-start" | "tool-end" | "compact-start" => {
            authority(RawAgentState::Working, state, Vec::new())
        }
        "permission-requested" | "question-asked" | "elicitation" => {
            state.pending_permission = true;
            authority(RawAgentState::Blocked, state, Vec::new())
        }
        "permission-resolved" | "question-answered" => {
            state.pending_permission = false;
            authority(RawAgentState::Working, state, Vec::new())
        }
        "turn-complete" | "stop" => {
            if state.pending_permission {
                authority(
                    RawAgentState::Blocked,
                    state,
                    vec![EvidenceNote::StopSuppressed],
                )
            } else if state.subagents_active > 0 {
                authority(
                    RawAgentState::Working,
                    state,
                    vec![EvidenceNote::StopSuppressed],
                )
            } else {
                authority(RawAgentState::Idle, state, Vec::new())
            }
        }
        "subagent-start" => {
            state.subagents_active = state.subagents_active.saturating_add(1);
            GateEffect::CounterOnly {
                subagents_active: state.subagents_active,
            }
        }
        "subagent-stop" => {
            state.subagents_active = state.subagents_active.saturating_sub(1);
            GateEffect::CounterOnly {
                subagents_active: state.subagents_active,
            }
        }
        "session-end" | "agent-exit" => GateEffect::Clear,
        "heartbeat" => GateEffect::Heartbeat,
        _ => GateEffect::Ignore,
    }
}

fn authority(
    state: RawAgentState,
    gate: &mut SourceGateState,
    notes: Vec<EvidenceNote>,
) -> GateEffect {
    GateEffect::Authority {
        state,
        pending_permission: gate.pending_permission,
        subagents_active: gate.subagents_active,
        notes,
    }
}

/// The canonical agent-event vocabulary. A reporter event already in canonical
/// form passes through unchanged; anything else falls to per-vendor mapping.
const CANONICAL_EVENTS: &[&str] = &[
    "prompt-submitted",
    "tool-start",
    "tool-end",
    "compact-start",
    "permission-requested",
    "question-asked",
    "elicitation",
    "permission-resolved",
    "question-answered",
    "turn-complete",
    "stop",
    "subagent-start",
    "subagent-stop",
    "session-end",
    "agent-exit",
    "heartbeat",
];

fn canonical_event(runtime: &str, event: &str) -> Option<&'static str> {
    let normalized = event.trim();
    CANONICAL_EVENTS
        .iter()
        .copied()
        .find(|canonical| *canonical == normalized)
        .or_else(|| canonical_vendor_event(runtime, normalized))
}

fn canonical_vendor_event(runtime: &str, event: &str) -> Option<&'static str> {
    match (runtime, event) {
        // Claude and Codex are identity-only authorities (Decision 0a): their
        // hook events are unreliable in order and timing (a SubagentStop/recap
        // can fire after the turn's Stop and would revive an idle pane), so they
        // never author working/blocked/idle. Every lifecycle event refreshes
        // freshness/liveness only; the screen rule pack + physics watchdog own
        // their state. Promoting any of these back to a state mapping reintroduces
        // the post-Stop revive hazard. Claude's SessionEnd is the one identity
        // exit edge that carries through; Codex emits no exit hook, so its exit is
        // detected via `/proc` process physics instead.
        ("claude", "SessionEnd") => Some("agent-exit"),
        (
            "claude" | "codex",
            "SessionStart" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse"
            | "PostToolUseFailure" | "PermissionRequest" | "PermissionDenied" | "Stop"
            | "StopFailure" | "SubagentStart" | "SubagentStop",
        ) => Some("heartbeat"),
        ("claude", e) if e.starts_with("Notification:") => Some("heartbeat"),
        ("opencode", "session.status" | "tool.execute.before") => Some("tool-start"),
        ("opencode", "tool.execute.after") => Some("tool-end"),
        ("opencode", "session.idle") => Some("turn-complete"),
        ("opencode", "permission.asked") => Some("permission-requested"),
        ("opencode", "permission.replied") => Some("permission-resolved"),
        ("opencode", "session.error") => Some("agent-exit"),
        ("amp", "agent.start" | "tool.call") => Some("tool-start"),
        ("amp", "tool.result") => Some("tool-end"),
        ("amp", "agent.end") => Some("turn-complete"),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
