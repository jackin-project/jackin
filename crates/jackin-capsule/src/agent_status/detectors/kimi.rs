//! Kimi screen-state detector.
//!
//! `--yolo` mode bypasses most tool approvals. Working = braille spinner;
//! blocked = approval-choice lines (not bypassed by yolo).

use super::{Detector, bottom_rows, contains_ci};
use crate::agent_status::AgentRawState;

/// Braille spinner glyphs Kimi cycles through while thinking.
const BRAILLE_SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

#[derive(Debug)]
pub struct KimiDetector;

impl Detector for KimiDetector {
    fn agent_slug(&self) -> Option<&str> {
        Some("kimi")
    }

    fn detect(&self, screen_rows: &[String]) -> Option<AgentRawState> {
        let rows = bottom_rows(screen_rows, super::DETECTION_ROWS);

        // Blocked: approval-choice prompt visible.
        let blocked = rows.iter().any(|l| {
            contains_ci(l, "approve once")
                || contains_ci(l, "approve for session")
                || (contains_ci(l, "reject") && contains_ci(l, "approve"))
        });
        if blocked {
            return Some(AgentRawState::BlockedVisible);
        }

        // Working: braille spinner glyph visible.
        let working = rows
            .iter()
            .any(|l| l.chars().any(|c| BRAILLE_SPINNER.contains(&c)));
        if working {
            return Some(AgentRawState::WorkingVisible);
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
    fn detects_working_from_braille_spinner() {
        let s = screen("⠹ thinking\r\n".as_bytes());
        assert_eq!(KimiDetector.detect(&s), Some(AgentRawState::WorkingVisible));
    }

    #[test]
    fn detects_blocked_from_approval_choices() {
        let s = screen(b"approve once  approve for session  reject\r\n");
        assert_eq!(KimiDetector.detect(&s), Some(AgentRawState::BlockedVisible));
    }
}
