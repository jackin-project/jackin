//! Codex screen-state detector.
//!
//! Patterns from Herdr's Codex detector. Idle prompt is `›`; working state
//! is a `• Working (` block marker; blocked is an explicit confirm prompt.

use super::{Detector, bottom_rows, contains_ci};
use crate::agent_status::AgentRawState;

#[derive(Debug)]
pub struct CodexDetector;

impl Detector for CodexDetector {
    fn agent_slug(&self) -> Option<&str> {
        Some("codex")
    }

    fn detect(&self, screen_rows: &[String]) -> Option<AgentRawState> {
        let rows = bottom_rows(screen_rows, super::DETECTION_ROWS);
        let joined = rows.join("\n");

        // Blocked: explicit confirmation / approval prompts (strong, as-is).
        if contains_ci(&joined, "press enter to confirm or esc to cancel")
            || contains_ci(&joined, "enter to submit answer")
            || contains_ci(&joined, "allow command?")
        {
            return Some(AgentRawState::BlockedVisible);
        }

        // Working: active block markers. Guard against installer/update
        // output (do not match those as working).
        let working = rows.iter().any(|line| {
            contains_ci(line, "• working (")
                || contains_ci(line, "waiting for background terminal (")
                || contains_ci(line, "esc to interrupt")
                || contains_ci(line, "/ps to view")
                || contains_ci(line, "booting mcp server:")
        });
        if working {
            return Some(AgentRawState::WorkingVisible);
        }

        // Idle: `›` prompt as the last non-empty line, no block marker below.
        let last = rows.iter().rev().find(|l| !l.is_empty());
        if let Some(last) = last
            && (last.starts_with('›') || last.trim_start().starts_with('›'))
        {
            return Some(AgentRawState::PromptVisible);
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
    fn detects_working_from_block_marker() {
        let s = screen("• Working (esc to interrupt)\r\n".as_bytes());
        assert_eq!(
            CodexDetector.detect(&s),
            Some(AgentRawState::WorkingVisible)
        );
    }

    #[test]
    fn detects_blocked_from_confirm_prompt() {
        let s = screen(b"press enter to confirm or esc to cancel\r\n");
        assert_eq!(
            CodexDetector.detect(&s),
            Some(AgentRawState::BlockedVisible)
        );
    }

    #[test]
    fn detects_idle_from_prompt() {
        let s = screen("› \r\n".as_bytes());
        assert_eq!(CodexDetector.detect(&s), Some(AgentRawState::PromptVisible));
    }

    #[test]
    fn installer_output_is_not_working() {
        let s = screen(b"Downloading codex update 1.2.3...\r\n");
        assert_eq!(CodexDetector.detect(&s), None);
    }
}
