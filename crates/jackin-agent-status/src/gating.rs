use crate::evidence::{EvidenceNote, RawAgentState};

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

/// Enrich a vendor hook event name using an optional JSON payload.
///
/// Claude Code installs hooks as bare `--event Notification`; the subtype
/// (`permission_prompt`, `idle_prompt`, `elicitation_*`) lives in the stdin
/// JSON. Without this enrichment, production maps bare `Notification` to
/// [`GateEffect::Ignore`] and never authors authority.
#[must_use]
pub fn enrich_event_name(runtime: &str, event: &str, payload: Option<&str>) -> String {
    if runtime == "claude" && event == "Notification" {
        if let Some(subtype) = claude_notification_subtype(payload) {
            return format!("Notification:{subtype}");
        }
    }
    event.to_owned()
}

fn claude_notification_subtype(payload: Option<&str>) -> Option<String> {
    let raw = payload?.trim();
    if raw.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    for key in ["notification_type", "type", "matcher"] {
        if let Some(subtype) = value.get(key).and_then(|v| v.as_str()) {
            let subtype = subtype.trim();
            if !subtype.is_empty() && subtype != "Notification" {
                return Some(subtype.to_owned());
            }
        }
    }
    None
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
        // Claude and Codex lifecycle hooks remain identity-only (Decision 0a):
        // their Stop/Subagent ordering can revive an idle pane after a turn ends.
        // Only documented wait-state notifications below author state.
        ("claude", "SessionEnd") => Some("agent-exit"),
        (
            "claude" | "codex",
            "SessionStart" | "UserPromptSubmit" | "PreToolUse" | "PostToolUse"
            | "PostToolUseFailure" | "PermissionRequest" | "PermissionDenied" | "Stop"
            | "StopFailure" | "SubagentStart" | "SubagentStop",
        ) => Some("heartbeat"),
        ("claude", "Notification:permission_prompt") => Some("permission-requested"),
        ("claude", "Notification:idle_prompt") => Some("turn-complete"),
        ("claude", e) if e.starts_with("Notification:elicitation_") => Some("elicitation"),
        ("claude", e) if e.starts_with("Notification:") => Some("heartbeat"),
        #[cfg(feature = "codex-app-server-authority")]
        ("codex-app-server", "turn/started") => Some("tool-start"),
        #[cfg(feature = "codex-app-server-authority")]
        ("codex-app-server", "turn/completed") => Some("turn-complete"),
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
