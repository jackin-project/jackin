//! Amp screen-state detector.
//!
//! Narrow patterns until stable fixtures captured. `--dangerously-allow-all`
//! bypasses most tool approvals, so blocked mostly means auth/question/modal
//! prompts. Prefer no signal over overfitting Amp text.

use vt100::Screen;

use super::{Detector, bottom_rows, contains_ci};
use crate::agent_status::AgentRawState;

pub struct AmpDetector;

impl Detector for AmpDetector {
    fn agent_slug(&self) -> Option<&str> {
        Some("amp")
    }

    fn detect(&self, screen: &Screen) -> Option<AgentRawState> {
        let rows = bottom_rows(screen, super::DETECTION_ROWS);

        // Working: interrupt chrome visible.
        if rows.iter().any(|l| contains_ci(l, "esc to cancel")) {
            return Some(AgentRawState::WorkingVisible);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use vt100::Parser;

    use super::*;

    fn screen(bytes: &[u8]) -> Screen {
        let mut p = Parser::new(10, 60, 0);
        p.process(bytes);
        p.screen().clone()
    }

    #[test]
    fn detects_working_from_cancel_chrome() {
        let s = screen(b"thinking...\r\nesc to cancel\r\n");
        assert_eq!(AmpDetector.detect(&s), Some(AgentRawState::WorkingVisible));
    }

    #[test]
    fn unknown_text_yields_none() {
        let s = screen(b"some output\r\n");
        assert_eq!(AmpDetector.detect(&s), None);
    }
}
