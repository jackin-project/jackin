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
#[doc(hidden)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Start a fresh capture: drop any prior lines and mark the sink active so the
/// command runner tees build output here.
pub fn begin() {
    if let Ok(mut lines) = LINES.lock() {
        lines.clear();
        // Flip the gate while holding the lock so a teeing writer never observes
        // the cleared-but-still-inactive window between reset and activation.
        ACTIVE.store(true, Ordering::Release);
    }
}

/// Stop teeing. The captured lines are retained so the dialog can still show
/// the finished log after the build completes.
pub fn end() {
    ACTIVE.store(false, Ordering::Release);
}

#[must_use]
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

/// Append one output line, dropping the oldest when the cap is reached.
pub fn push_line(line: &str) {
    if let Ok(mut lines) = LINES.lock() {
        if lines.len() >= MAX_LINES {
            lines.pop_front();
        }
        lines.push_back(line.to_owned());
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

#[cfg(test)]
mod tests;
