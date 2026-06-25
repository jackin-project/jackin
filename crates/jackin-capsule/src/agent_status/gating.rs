use crate::agent_status::evidence::{EvidenceNote, RawAgentState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEvent {
    pub runtime: String,
    pub event: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceGateState {
    pub pending_permission: bool,
    pub subagents_active: u32,
    pub notes: Vec<EvidenceNote>,
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

pub fn map_event(event: &RuntimeEvent, state: &mut SourceGateState) -> GateEffect {
    let Some(canonical) = canonical_event(event.runtime.as_str(), event.event.as_str()) else {
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
    gate.notes = notes.clone();
    GateEffect::Authority {
        state,
        pending_permission: gate.pending_permission,
        subagents_active: gate.subagents_active,
        notes,
    }
}

fn canonical_event(runtime: &str, event: &str) -> Option<&'static str> {
    let normalized = event.trim();
    match normalized {
        "prompt-submitted" => Some("prompt-submitted"),
        "tool-start" => Some("tool-start"),
        "tool-end" => Some("tool-end"),
        "compact-start" => Some("compact-start"),
        "permission-requested" => Some("permission-requested"),
        "question-asked" => Some("question-asked"),
        "elicitation" => Some("elicitation"),
        "permission-resolved" => Some("permission-resolved"),
        "question-answered" => Some("question-answered"),
        "turn-complete" => Some("turn-complete"),
        "stop" => Some("stop"),
        "subagent-start" => Some("subagent-start"),
        "subagent-stop" => Some("subagent-stop"),
        "session-end" => Some("session-end"),
        "agent-exit" => Some("agent-exit"),
        "heartbeat" => Some("heartbeat"),
        _ => canonical_vendor_event(runtime, normalized),
    }
}

fn canonical_vendor_event(runtime: &str, event: &str) -> Option<&'static str> {
    match (runtime, event) {
        ("claude" | "codex", "UserPromptSubmit") => Some("prompt-submitted"),
        ("claude" | "codex", "PreToolUse") => Some("tool-start"),
        ("claude" | "codex", "PostToolUse") | ("claude", "PostToolUseFailure") => Some("tool-end"),
        ("claude" | "codex", "PermissionRequest") => Some("permission-requested"),
        ("claude", "PermissionDenied") => Some("permission-resolved"),
        ("claude", "Notification:permission_prompt" | "Notification:elicitation_dialog") => {
            Some("permission-requested")
        }
        ("claude", "Notification:idle_prompt" | "Notification:auth_success") => Some("heartbeat"),
        ("claude" | "codex", "Stop") | ("claude", "StopFailure") => Some("turn-complete"),
        ("claude" | "codex", "SubagentStart") => Some("subagent-start"),
        ("claude" | "codex", "SubagentStop") => Some("subagent-stop"),
        ("claude", "SessionEnd") => Some("agent-exit"),
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
