//! OpenCode screen-state detector.
//!
//! OpenCode uses Bubble Tea (Go), which continuously redraws — output
//! silence is never a reliable working signal. Match only explicit visible
//! affordances. The OpenCode API/SSE bridge (Phase 3) is the preferred
//! source; this is the direct-PTY fallback.

use vt100::Screen;

use super::{Detector, bottom_rows, contains_ci};
use crate::agent_status::AgentRawState;

pub struct OpenCodeDetector;

impl Detector for OpenCodeDetector {
    fn agent_slug(&self) -> Option<&str> {
        Some("opencode")
    }

    fn detect(&self, screen: &Screen) -> Option<AgentRawState> {
        let rows = bottom_rows(screen, super::DETECTION_ROWS);

        // Blocked: permission-required UI / question prompt.
        let blocked = rows.iter().any(|l| {
            contains_ci(l, "permission required")
                || (contains_ci(l, "dismiss") && contains_ci(l, "select"))
        });
        if blocked {
            return Some(AgentRawState::BlockedVisible);
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
    fn detects_blocked_from_permission_ui() {
        let s = screen(b"permission required to run command\r\n");
        assert_eq!(
            OpenCodeDetector.detect(&s),
            Some(AgentRawState::BlockedVisible)
        );
    }

    #[test]
    fn plain_output_yields_none() {
        let s = screen(b"some bubble tea redraw\r\n");
        assert_eq!(OpenCodeDetector.detect(&s), None);
    }
}
