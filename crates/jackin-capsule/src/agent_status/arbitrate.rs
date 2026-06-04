//! Pure arbitration function for agent state authority.
//!
//! `arbitrate_session_status` is a side-effect-free function that
//! consumes evidence from all signal sources and returns the best-
//! confidence raw state. Called by the daemon's 1Hz ticker.

use std::time::{Duration, Instant};

use crate::agent_status::HookAuthority;
use crate::protocol::AgentState;

/// Evidence from the visible terminal screen for one session.
#[derive(Debug, Default, Clone)]
pub struct ScreenDetection {
    /// An explicit approval/input-required prompt is currently visible.
    pub visible_blocker: bool,
    /// Working chrome (spinner, interrupt hint, token stats) is visible.
    pub visible_working: bool,
    /// Idle prompt box is currently visible and stable.
    pub visible_idle: bool,
    /// When the screen observation was taken.
    pub observed_at: Option<Instant>,
}

/// Evidence from the foreground process group for one session.
#[derive(Debug, Default, Clone)]
pub struct ProcessEvidence {
    /// The agent process has exited.
    pub process_exited: bool,
    /// Detected agent slug (e.g. "claude", "codex").
    pub detected_agent: Option<String>,
}

/// Confidence tier for the arbitrated result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatusConfidence {
    Unknown,
    Weak,
    Strong,
    Authoritative,
}

/// Arbitrate the effective raw state from all available evidence.
///
/// Priority (highest first):
/// 1. visible_blocker AND no hook OR hook agrees → `Blocked, Authoritative`
/// 2. visible_blocker overrides non-blocked hook → `Blocked, Strong`
/// 3. visible_working overrides hook Idle/Blocked (screen fresher) → `Working, Strong`
/// 4. visible_idle stales hook Working/Blocked after 2s → `Idle, Strong`
/// 5. fresh hook authority (process-consistent, sequence valid) → hook state, Authoritative
/// 6. screen fallback (visible signal) → screen state, Strong
/// 7. process alive but no signals → `Working, Weak` (conservative)
/// 8. default → `Unknown, Unknown`
pub fn arbitrate_session_status(
    hook: Option<&HookAuthority>,
    screen: &ScreenDetection,
    process: &ProcessEvidence,
    now: Instant,
) -> (AgentState, StatusConfidence) {
    const STALE_HOOK_IDLE_GRACE: Duration = Duration::from_secs(2);

    // Process exit overrides everything.
    if process.process_exited {
        return (AgentState::Idle, StatusConfidence::Weak);
    }

    // 1 & 2. Visible blocker is the highest screen override.
    if screen.visible_blocker {
        if let Some(h) = hook {
            let hook_age = now.duration_since(h.last_seen);
            let screen_is_fresh = screen.observed_at
                .map(|t| now.duration_since(t) < hook_age)
                .unwrap_or(true);
            if h.raw_state == "blocked" {
                return (AgentState::Blocked, StatusConfidence::Authoritative);
            } else if screen_is_fresh {
                return (AgentState::Blocked, StatusConfidence::Strong);
            }
        } else {
            return (AgentState::Blocked, StatusConfidence::Strong);
        }
    }

    // 3. Visible working overrides hook Idle or Blocked.
    if screen.visible_working {
        if let Some(h) = hook {
            if matches!(h.raw_state.as_str(), "idle" | "blocked") {
                let hook_age = now.duration_since(h.last_seen);
                let screen_fresh = screen.observed_at
                    .map(|t| now.duration_since(t) < hook_age)
                    .unwrap_or(true);
                if screen_fresh {
                    return (AgentState::Working, StatusConfidence::Strong);
                }
            }
        } else {
            return (AgentState::Working, StatusConfidence::Strong);
        }
    }

    // 4. Visible idle stales a working/blocked hook after grace period.
    if screen.visible_idle {
        if let Some(h) = hook {
            if matches!(h.raw_state.as_str(), "working" | "blocked") {
                let idle_duration = screen.observed_at
                    .map(|t| now.duration_since(t))
                    .unwrap_or(Duration::ZERO);
                if idle_duration >= STALE_HOOK_IDLE_GRACE {
                    return (AgentState::Idle, StatusConfidence::Strong);
                }
            }
        } else {
            return (AgentState::Idle, StatusConfidence::Strong);
        }
    }

    // 5. Fresh hook authority.
    if let Some(h) = hook {
        let consistent = process.detected_agent.as_deref()
            .map(|a| a == h.agent_label || h.agent_label.is_empty())
            .unwrap_or(true);
        if consistent {
            let state = match h.raw_state.as_str() {
                "working" => AgentState::Working,
                "blocked" => AgentState::Blocked,
                "idle"    => AgentState::Idle,
                _         => AgentState::Unknown,
            };
            return (state, StatusConfidence::Authoritative);
        }
    }

    // 6. Screen fallback.
    if screen.visible_working {
        return (AgentState::Working, StatusConfidence::Strong);
    }
    if screen.visible_idle {
        return (AgentState::Idle, StatusConfidence::Strong);
    }

    // 7. Process alive, no screen signals — conservatively working.
    if process.detected_agent.is_some() {
        return (AgentState::Working, StatusConfidence::Weak);
    }

    // 8. Nothing.
    (AgentState::Unknown, StatusConfidence::Unknown)
}

/// Attention priority used for tab/workspace roll-up.
pub fn attention_priority(state: AgentState) -> u8 {
    match state {
        AgentState::Blocked => 4,
        AgentState::Done    => 3,
        AgentState::Working => 2,
        AgentState::Idle    => 1,
        AgentState::Unknown => 0,
    }
}

/// Roll up a collection of session states to the most attention-worthy.
pub fn roll_up_states<'a>(states: impl IntoIterator<Item = &'a AgentState>) -> AgentState {
    states
        .into_iter()
        .max_by_key(|&&s| attention_priority(s))
        .copied()
        .unwrap_or(AgentState::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use crate::agent_status::HookAuthority;

    fn hook(raw_state: &str) -> HookAuthority {
        HookAuthority {
            source_id: "test".to_string(),
            agent_label: "claude".to_string(),
            raw_state: raw_state.to_string(),
            seq: 1,
            ts_ns: 0,
            message: None,
            last_seen: Instant::now(),
        }
    }

    fn screen_blocker() -> ScreenDetection {
        ScreenDetection { visible_blocker: true, observed_at: Some(Instant::now()), ..Default::default() }
    }
    fn screen_working() -> ScreenDetection {
        ScreenDetection { visible_working: true, observed_at: Some(Instant::now()), ..Default::default() }
    }
    #[allow(dead_code)]
    fn screen_idle() -> ScreenDetection {
        ScreenDetection { visible_idle: true, observed_at: Some(Instant::now()), ..Default::default() }
    }

    #[test]
    fn arbitrate_unknown_when_no_signals() {
        let (state, conf) = arbitrate_session_status(None, &ScreenDetection::default(), &ProcessEvidence::default(), Instant::now());
        assert_eq!(state, AgentState::Unknown);
        assert_eq!(conf, StatusConfidence::Unknown);
    }

    #[test]
    fn arbitrate_working_from_process_alive() {
        let proc = ProcessEvidence { detected_agent: Some("claude".to_string()), ..Default::default() };
        let (state, conf) = arbitrate_session_status(None, &ScreenDetection::default(), &proc, Instant::now());
        assert_eq!(state, AgentState::Working);
        assert_eq!(conf, StatusConfidence::Weak);
    }

    #[test]
    fn arbitrate_blocked_from_screen_blocker_no_hook() {
        let (state, conf) = arbitrate_session_status(None, &screen_blocker(), &ProcessEvidence::default(), Instant::now());
        assert_eq!(state, AgentState::Blocked);
        assert_eq!(conf, StatusConfidence::Strong);
    }

    #[test]
    fn arbitrate_blocked_authoritative_when_hook_agrees() {
        let h = hook("blocked");
        let (state, conf) = arbitrate_session_status(Some(&h), &screen_blocker(), &ProcessEvidence::default(), Instant::now());
        assert_eq!(state, AgentState::Blocked);
        assert_eq!(conf, StatusConfidence::Authoritative);
    }

    #[test]
    fn arbitrate_working_overrides_idle_hook_with_fresher_screen() {
        let mut h = hook("idle");
        // Make hook appear stale (old last_seen).
        h.last_seen = Instant::now() - std::time::Duration::from_secs(5);
        let (state, conf) = arbitrate_session_status(Some(&h), &screen_working(), &ProcessEvidence::default(), Instant::now());
        assert_eq!(state, AgentState::Working);
        assert_eq!(conf, StatusConfidence::Strong);
    }

    #[test]
    fn arbitrate_fresh_hook_authority_wins() {
        let h = hook("blocked");
        let (state, conf) = arbitrate_session_status(Some(&h), &ScreenDetection::default(), &ProcessEvidence::default(), Instant::now());
        assert_eq!(state, AgentState::Blocked);
        assert_eq!(conf, StatusConfidence::Authoritative);
    }

    #[test]
    fn arbitrate_process_exit_clears_to_idle() {
        let h = hook("blocked");
        let proc = ProcessEvidence { process_exited: true, ..Default::default() };
        let (state, _) = arbitrate_session_status(Some(&h), &ScreenDetection::default(), &proc, Instant::now());
        assert_eq!(state, AgentState::Idle);
    }

    #[test]
    fn arbitrate_hook_cleared_when_wrong_agent() {
        let h = HookAuthority {
            source_id: "hook-1".to_string(),
            agent_label: "claude".to_string(),
            raw_state: "working".to_string(),
            seq: 1,
            ts_ns: 0,
            message: None,
            last_seen: Instant::now(),
        };
        let proc = ProcessEvidence {
            detected_agent: Some("codex".to_string()),
            ..Default::default()
        };
        // Hook is for Claude but Codex is foreground — hook consistency check
        // fails, falls back to process evidence.
        let (state, conf) = arbitrate_session_status(Some(&h), &ScreenDetection::default(), &proc, Instant::now());
        // Process alive (Codex) → Working, Weak (hook cleared by inconsistency)
        assert_eq!(state, AgentState::Working);
        assert_eq!(conf, StatusConfidence::Weak);
    }

    #[test]
    fn roll_up_blocked_beats_working_beats_idle() {
        let states = vec![AgentState::Idle, AgentState::Working, AgentState::Blocked, AgentState::Done];
        assert_eq!(roll_up_states(&states), AgentState::Blocked);
    }

    #[test]
    fn roll_up_done_beats_working() {
        let states = vec![AgentState::Working, AgentState::Done];
        assert_eq!(roll_up_states(&states), AgentState::Done);
    }

    #[test]
    fn roll_up_unknown_when_empty() {
        let states: Vec<AgentState> = vec![];
        assert_eq!(roll_up_states(&states), AgentState::Unknown);
    }

    #[test]
    fn attention_priority_order() {
        assert!(attention_priority(AgentState::Blocked) > attention_priority(AgentState::Done));
        assert!(attention_priority(AgentState::Done) > attention_priority(AgentState::Working));
        assert!(attention_priority(AgentState::Working) > attention_priority(AgentState::Idle));
        assert!(attention_priority(AgentState::Idle) > attention_priority(AgentState::Unknown));
    }
}
