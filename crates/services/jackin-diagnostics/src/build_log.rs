// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::redact;

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

pub fn begin() {
    if let Ok(mut state) = state().lock() {
        state.lines.clear();
        state.redactor.reset();
        ACTIVE.store(true, Ordering::Release);
    }
}

pub fn end() {
    ACTIVE.store(false, Ordering::Release);
    if let Ok(mut state) = state().lock() {
        state.redactor.reset();
    }
}

#[must_use]
pub fn is_active() -> bool {
    ACTIVE.load(Ordering::Acquire)
}

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

#[must_use]
pub fn len() -> usize {
    state().lock().map_or(0, |state| state.lines.len())
}

#[must_use]
pub fn snapshot() -> Vec<String> {
    state().lock().map_or_else(
        |_| Vec::new(),
        |state| state.lines.iter().cloned().collect(),
    )
}

#[cfg(test)]
mod tests;
