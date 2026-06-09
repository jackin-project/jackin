//! OpenCode screen-state detector.
//!
//! OpenCode uses Bubble Tea (Go), which continuously redraws — output
//! silence is never a reliable working signal. Match only explicit visible
//! affordances. The OpenCode ACP bridge (Phase 3) is the preferred
//! source; this is the direct-PTY fallback.

use super::{Detector, bottom_rows, contains_ci};
use crate::agent_status::AgentRawState;

#[derive(Debug)]
pub struct OpenCodeDetector;

impl Detector for OpenCodeDetector {
    fn agent_slug(&self) -> Option<&str> {
        Some("opencode")
    }

    fn detect(&self, screen_rows: &[String]) -> Option<AgentRawState> {
        let rows = bottom_rows(screen_rows, super::DETECTION_ROWS);

        // Blocked: permission-required UI / question prompt.
        let blocked = rows.iter().any(|l| {
            contains_ci(l, "permission required")
                || (contains_ci(l, "dismiss")
                    && (contains_ci(l, "select") || rows.iter().any(|r| contains_ci(r, "enter"))))
        });
        if blocked {
            return Some(AgentRawState::BlockedVisible);
        }

        // Working: Bubble Tea interrupt chrome (Ctrl+C to cancel style).
        if rows.iter().any(|l| {
            contains_ci(l, "ctrl+c to cancel")
                || contains_ci(l, "ctrl-c to cancel")
                || (contains_ci(l, "interrupt") && contains_ci(l, "cancel"))
        }) {
            return Some(AgentRawState::WorkingVisible);
        }

        // Idle: input box visible at bottom (Bubble Tea prompt area).
        // OpenCode renders an input box at the bottom when waiting for input.
        let last_nonempty = rows.iter().rev().find(|l| !l.trim().is_empty());
        if let Some(line) = last_nonempty {
            let trimmed = line.trim();
            if trimmed.starts_with("> ") || trimmed == ">" || trimmed.starts_with("❯") {
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
    fn detects_blocked_from_permission_ui() {
        let s = screen(b"permission required to run command\r\n");
        assert_eq!(
            OpenCodeDetector.detect(&s),
            Some(AgentRawState::BlockedVisible)
        );
    }

    #[test]
    fn detects_blocked_from_question_prompt() {
        let s = screen(b"File will be overwritten\r\ndismiss  enter  select\r\n");
        assert_eq!(
            OpenCodeDetector.detect(&s),
            Some(AgentRawState::BlockedVisible)
        );
    }

    #[test]
    fn detects_working_from_interrupt_chrome() {
        let s = screen(b"Processing your request...\r\nCtrl+C to cancel\r\n");
        assert_eq!(
            OpenCodeDetector.detect(&s),
            Some(AgentRawState::WorkingVisible)
        );
    }

    #[test]
    fn detects_idle_from_input_box() {
        let s = screen(b"OpenCode ready.\r\n> \r\n");
        assert_eq!(
            OpenCodeDetector.detect(&s),
            Some(AgentRawState::PromptVisible)
        );
    }

    #[test]
    fn plain_output_yields_none() {
        let s = screen(b"some bubble tea redraw\r\n");
        assert_eq!(OpenCodeDetector.detect(&s), None);
    }
}
