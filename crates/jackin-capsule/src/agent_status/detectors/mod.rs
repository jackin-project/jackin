//! Screen-based agent state detectors.
//!
//! Each built-in runtime has a dedicated [`Detector`] implementation that
//! inspects the bottom rows of the current `vt100::Screen` and returns an
//! [`AgentRawState`] observation, or `None` when the screen contains no
//! confident signal.
//!
//! # Extension
//!
//! To add support for a new agent:
//! 1. Create `<slug>.rs` in this module implementing [`Detector`].
//! 2. Register it in [`default_registry`] — one line.
//! 3. No changes to the state machine, daemon, or session code.

use std::collections::HashMap;

use vt100::Screen;

use super::AgentRawState;

pub mod amp;
pub mod claude;
pub mod codex;
pub mod kimi;
pub mod opencode;

/// Maximum number of rows from the bottom of the visible viewport that a
/// detector inspects. Matches Herdr's `DEFAULT_DETECTION_ROWS = 24`.
pub const DETECTION_ROWS: u16 = 24;

/// Interface for a runtime-specific screen-state detector.
///
/// Detectors are pure functions over `&vt100::Screen` — no async, no I/O,
/// no timers. This keeps them fast (called on every 1Hz tick) and trivially
/// testable without a live PTY.
pub trait Detector: Send + Sync + 'static {
    /// Agent slug this detector claims, e.g. `"claude"`. Shell sessions
    /// should use `None`; they are matched against the `None` key in the
    /// registry.
    fn agent_slug(&self) -> Option<&str>;

    /// Inspect `screen` and return a raw state signal, or `None` when no
    /// confident pattern is found in the visible content.
    ///
    /// Implementors MUST only inspect the bottom [`DETECTION_ROWS`] rows of
    /// the viewport and MUST NOT scan historical scrollback — only the
    /// current visible terminal UI is authoritative.
    fn detect(&self, screen: &Screen) -> Option<AgentRawState>;
}

/// Registry of all known detectors. Keyed on `Option<String>` (agent slug
/// or `None` for shell sessions). Only one detector per agent is supported.
pub struct DetectorRegistry {
    detectors: HashMap<Option<String>, Box<dyn Detector>>,
}

impl DetectorRegistry {
    /// Construct a registry with all built-in detectors registered.
    pub fn default_registry() -> Self {
        let mut r = Self {
            detectors: HashMap::new(),
        };
        r.register(Box::new(claude::ClaudeDetector));
        r.register(Box::new(codex::CodexDetector));
        r.register(Box::new(amp::AmpDetector));
        r.register(Box::new(kimi::KimiDetector));
        r.register(Box::new(opencode::OpenCodeDetector));
        r
    }

    fn register(&mut self, d: Box<dyn Detector>) {
        self.detectors.insert(d.agent_slug().map(str::to_string), d);
    }

    /// Run the detector for `agent` against `screen`. Returns `None` when
    /// no detector is registered for this agent or when the registered
    /// detector finds no signal.
    pub fn detect(&self, agent: Option<&str>, screen: &Screen) -> Option<AgentRawState> {
        self.detectors
            .get(&agent.map(str::to_string))?
            .detect(screen)
    }
}

/// Helper: collect the text of the bottom N rows of the screen, trimmed of
/// trailing whitespace on each line. Returns lines in top-to-bottom order.
pub(crate) fn bottom_rows(screen: &Screen, n: u16) -> Vec<String> {
    let (rows, cols) = screen.size();
    let start = rows.saturating_sub(n);
    (start..rows)
        .map(|r| {
            let mut line = String::with_capacity(cols as usize);
            for c in 0..cols {
                if let Some(cell) = screen.cell(r, c) {
                    line.push_str(cell.contents());
                }
            }
            line.trim_end().to_string()
        })
        .collect()
}

/// Helper: return `true` if `text` contains `pattern` as a substring
/// (case-insensitive ASCII comparison).
pub(crate) fn contains_ci(text: &str, pattern: &str) -> bool {
    // Fast path for patterns likely to appear in hot code.
    let t = text.to_ascii_lowercase();
    let p = pattern.to_ascii_lowercase();
    t.contains(p.as_str())
}

#[cfg(test)]
mod tests {
    use vt100::Parser;

    use super::*;

    fn screen_from_bytes(rows: u16, cols: u16, bytes: &[u8]) -> vt100::Screen {
        let mut p = Parser::new(rows, cols, 0);
        p.process(bytes);
        p.screen().clone()
    }

    #[test]
    fn bottom_rows_returns_visible_lines() {
        // 3 visible rows, write two lines.
        let screen = screen_from_bytes(3, 20, b"line one\r\nline two\r\n");
        let rows = bottom_rows(&screen, 3);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|r| r.contains("line one")));
        assert!(rows.iter().any(|r| r.contains("line two")));
    }

    #[test]
    fn registry_returns_none_for_unknown_agent() {
        let reg = DetectorRegistry::default_registry();
        let screen = screen_from_bytes(5, 40, b"");
        assert_eq!(reg.detect(Some("gemini"), &screen), None);
    }
}
