// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Product-local terminal ownership and title policy.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

pub use jackin_core::shorten_home;

static RICH_SURFACE_ACTIVE: AtomicBool = AtomicBool::new(false);
static HOST_SCREEN_OWNED: AtomicBool = AtomicBool::new(false);

pub fn set_rich_surface_active(active: bool) {
    RICH_SURFACE_ACTIVE.store(active, Ordering::Relaxed);
}
#[must_use]
pub fn rich_surface_active() -> bool {
    RICH_SURFACE_ACTIVE.load(Ordering::Relaxed)
}
pub fn set_host_screen_owned(owned: bool) {
    HOST_SCREEN_OWNED.store(owned, Ordering::Relaxed);
}
#[must_use]
pub fn host_screen_owned() -> bool {
    HOST_SCREEN_OWNED.load(Ordering::Relaxed)
}
#[must_use]
pub fn rich_terminal_owned() -> bool {
    rich_surface_active() || host_screen_owned()
}

pub fn reassert_alt_screen() {
    use crossterm::ExecutableCommand as _;
    if !host_screen_owned() {
        return;
    }
    let mut out = io::stdout();
    drop(out.execute(crossterm::terminal::EnterAlternateScreen));
    drop(out.execute(crossterm::cursor::Hide));
}

pub fn set_terminal_title(title: &str) {
    let mut stderr = io::stderr().lock();
    drop(write!(stderr, "\x1b]0;jackin❯ · {title}\x07"));
    drop(stderr.flush());
}
