//! Live docker-build output sink.
//!
//! The derived-image `docker build` is the slowest launch step. The command
//! runner tees its captured output here line-by-line so the loading cockpit
//! can show a live, scrollable view on demand. Keeping it in a process-global
//! buffer decouples the generic command runner (which knows nothing about the
//! cockpit) from the cockpit's view state (which knows nothing about docker).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Cap on retained lines. A long `BuildKit` run is bounded so the buffer
/// cannot grow without limit; the oldest lines drop first.
const MAX_LINES: usize = 5000;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static LINES: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());

/// Start a fresh capture: drop any prior lines and mark the sink active so the
/// command runner tees build output here.
pub fn begin() {
    if let Ok(mut lines) = LINES.lock() {
        lines.clear();
    }
    ACTIVE.store(true, Ordering::Relaxed);
}

/// Stop teeing. The captured lines are retained so the dialog can still show
/// the finished log after the build completes.
pub fn end() {
    ACTIVE.store(false, Ordering::Relaxed);
}

#[must_use]
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Append one output line, dropping the oldest when the cap is reached.
pub fn push_line(line: &str) {
    if let Ok(mut lines) = LINES.lock() {
        if lines.len() >= MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(line.to_string());
    }
}

/// Number of retained lines, for scroll math without cloning the buffer.
#[must_use]
pub fn len() -> usize {
    LINES.lock().map_or(0, |lines| lines.len())
}

/// Snapshot the retained lines for rendering.
#[must_use]
pub fn snapshot() -> Vec<String> {
    LINES
        .lock()
        .map_or_else(|_| Vec::new(), |lines| lines.iter().cloned().collect())
}

/// The visible window of lines for a viewport `height` rows tall.
///
/// `scroll` counts lines up from the tail (`0` follows the newest output).
/// Clones only the visible lines, not the whole buffer — the cockpit calls
/// this every render frame while the overlay is open.
#[must_use]
pub fn window_from_bottom(scroll: usize, height: usize) -> Vec<String> {
    let Ok(lines) = LINES.lock() else {
        return Vec::new();
    };
    let top = lines
        .len()
        .saturating_sub(height)
        .saturating_sub(scroll);
    lines.iter().skip(top).take(height).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test only: the buffer is a process-global static, so splitting these
    // assertions into parallel tests would race on the shared state.
    #[test]
    fn buffer_caps_windows_and_clears() {
        begin();
        for i in 0..(MAX_LINES + 10) {
            push_line(&format!("line {i}"));
        }
        assert_eq!(len(), MAX_LINES);
        let snap = snapshot();
        assert_eq!(snap.first().map(String::as_str), Some("line 10"));
        assert_eq!(
            snap.last().map(String::as_str),
            Some(&*format!("line {}", MAX_LINES + 9))
        );

        // window_from_bottom on a known buffer: tail-follow, scroll, and a
        // buffer smaller than the viewport.
        begin();
        for i in 0..100 {
            push_line(&format!("line {i}"));
        }
        assert_eq!(
            window_from_bottom(0, 5),
            vec!["line 95", "line 96", "line 97", "line 98", "line 99"]
        );
        assert_eq!(
            window_from_bottom(10, 5),
            vec!["line 85", "line 86", "line 87", "line 88", "line 89"]
        );
        begin();
        push_line("only");
        assert_eq!(window_from_bottom(0, 5), vec!["only"]);

        end();
        assert!(!is_active());
    }
}
