// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Live docker-build output sink.
//!
//! The derived-image `docker build` is the slowest launch step. The command
//! runner tees its captured output here line-by-line so the loading cockpit
//! can show a live, scrollable view on demand. Keeping it in a process-global
//! buffer decouples the generic command runner (which knows nothing about the
//! cockpit) from the cockpit's view state (which knows nothing about docker).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::redact;

/// Cap on retained lines. A long `BuildKit` run is bounded so the buffer
/// cannot grow without limit; the oldest lines drop first.
const MAX_LINES: usize = 5000;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static STATE: OnceLock<Mutex<BuildLogState>> = OnceLock::new();
#[doc(hidden)]
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

struct BuildLogState {
    lines: VecDeque<String>,
    redactor: redact::StreamRedactor,
}

fn state() -> &'static Mutex<BuildLogState> {
    STATE.get_or_init(|| {
        Mutex::new(BuildLogState {
            lines: VecDeque::new(),
            redactor: redact::StreamRedactor::default(),
        })
    })
}

/// Start a fresh capture: drop any prior lines and mark the sink active so the
/// command runner tees build output here.
pub fn begin() {
    if let Ok(mut state) = state().lock() {
        state.lines.clear();
        state.redactor.reset();
        // Flip the gate while holding the lock so a teeing writer never observes
        // the cleared-but-still-inactive window between reset and activation.
        ACTIVE.store(true, Ordering::Release);
    }
}

/// Stop teeing. The captured lines are retained so the dialog can still show
/// the finished log after the build completes.
pub fn end() {
    ACTIVE.store(false, Ordering::Release);
    if let Ok(mut state) = state().lock() {
        // Discard any unterminated secret context so a later capture cannot
        // inherit it. `finish` returns no body text for an open context.
        drop(state.redactor.finish());
    }
}

#[must_use]
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

/// Append one output line, dropping the oldest when the cap is reached.
///
/// Secret contexts persist across calls until a record closes them or the
/// capture ends. Callers multiplexing stdout and stderr must first use one
/// `StreamRedactor` per source stream so this sink never has to guess which
/// pipe owns an unframed continuation.
pub fn push_line(line: &str) {
    if let Ok(mut state) = state().lock() {
        let line = state.redactor.push_complete_text(line);
        if line.is_empty() {
            return;
        }
        if state.lines.len() >= MAX_LINES {
            state.lines.pop_front();
        }
        state.lines.push_back(line);
    }
}

/// Number of retained lines, for scroll math without cloning the buffer.
#[must_use]
pub fn len() -> usize {
    state().lock().map_or(0, |state| state.lines.len())
}

/// Snapshot the retained lines for rendering.
#[must_use]
pub fn snapshot() -> Vec<String> {
    state().lock().map_or_else(
        |_| Vec::new(),
        |state| state.lines.iter().cloned().collect(),
    )
}

#[cfg(test)]
mod tests;
