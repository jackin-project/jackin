//! Amp screen-state detector.
//!
//! Narrow patterns until stable fixtures captured. `--dangerously-allow-all`
//! bypasses most tool approvals, so blocked mostly means auth/question/modal
//! prompts. Prefer no signal over overfitting Amp text.

use super::{Detector, bottom_rows, contains_ci};
use crate::agent_status::AgentRawState;

#[derive(Debug)]
pub struct AmpDetector;

impl Detector for AmpDetector {
    fn agent_slug(&self) -> Option<&str> {
        Some("amp")
    }

    fn detect(&self, screen_rows: &[String]) -> Option<AgentRawState> {
        let rows = bottom_rows(screen_rows, super::DETECTION_ROWS);

        // Blocked: explicit question/approval prompt with input affordance.
        // Amp shows "Allow?" or "Approve" headers with an escape affordance.
        let blocked = rows.iter().any(|l| {
            contains_ci(l, "allow?")
                || contains_ci(l, "approve") && rows.iter().any(|r| contains_ci(r, "esc to cancel"))
                || (contains_ci(l, "allow") && contains_ci(l, "deny"))
        });
        if blocked {
            return Some(AgentRawState::BlockedVisible);
        }

        // Working: interrupt chrome visible.
        if rows.iter().any(|l| contains_ci(l, "esc to cancel")) {
            return Some(AgentRawState::WorkingVisible);
        }

        // Idle: clean prompt line at the bottom with no working/blocked chrome.
        // Amp shows a `>` prompt or an input box when idle.
        let last_nonempty = rows.iter().rev().find(|l| !l.trim().is_empty());
        if let Some(line) = last_nonempty {
            let trimmed = line.trim();
            if trimmed == ">" || trimmed.starts_with("> ") || trimmed.starts_with("❯") {
                return Some(AgentRawState::PromptVisible);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(bytes: &[u8]) -> Vec<String> {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn detects_working_from_cancel_chrome() {
        let s = screen(b"thinking...\r\nesc to cancel\r\n");
        assert_eq!(AmpDetector.detect(&s), Some(AgentRawState::WorkingVisible));
    }

    #[test]
    fn detects_blocked_from_approval_prompt() {
        let s = screen(b"Amp wants to run: rm -rf /tmp\r\nAllow?  esc to cancel\r\n");
        assert_eq!(AmpDetector.detect(&s), Some(AgentRawState::BlockedVisible));
    }

    #[test]
    fn detects_idle_from_prompt_line() {
        let s = screen(b"Previous output\r\n>\r\n");
        assert_eq!(AmpDetector.detect(&s), Some(AgentRawState::PromptVisible));
    }

    #[test]
    fn detects_idle_from_arrow_prompt() {
        let s = screen(b"Ready for your task.\r\n> \r\n");
        assert_eq!(AmpDetector.detect(&s), Some(AgentRawState::PromptVisible));
    }

    #[test]
    fn unknown_text_yields_none() {
        let s = screen(b"some output\r\n");
        assert_eq!(AmpDetector.detect(&s), None);
    }
}
