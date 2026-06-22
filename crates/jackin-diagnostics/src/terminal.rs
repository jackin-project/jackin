//! Terminal-ownership flags, alt-screen assertion, and terminal-title helpers.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static RICH_SURFACE_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Set while a full-screen rich TUI owns the alternate screen.
///
/// Ancillary stderr status output — spinners, "waiting" lines — checks this
/// and stays silent so it cannot stream over the cockpit. Driven by the
/// renderer's lifetime, never by callers.
pub fn set_rich_surface_active(active: bool) {
    RICH_SURFACE_ACTIVE.store(active, Ordering::Relaxed);
}

#[must_use]
pub fn rich_surface_active() -> bool {
    RICH_SURFACE_ACTIVE.load(Ordering::Relaxed)
}

static HOST_SCREEN_OWNED: AtomicBool = AtomicBool::new(false);

/// Set while a single host-side guard owns the screen for a whole launch flow.
///
/// The guard holds the alternate screen, raw mode, and mouse capture across
/// console → loading → capsule → exit. The individual surfaces (console
/// manager, launch cockpit, exit outro) check this and skip their own
/// enter/leave so the flow never drops back to the cooked terminal between
/// screens. Driven only by the owning guard's lifetime.
pub fn set_host_screen_owned(owned: bool) {
    HOST_SCREEN_OWNED.store(owned, Ordering::Relaxed);
}

#[must_use]
pub fn host_screen_owned() -> bool {
    HOST_SCREEN_OWNED.load(Ordering::Relaxed)
}

/// True when any host-side full-screen surface owns terminal modes that make
/// direct stdout/stderr streaming unsafe.
///
/// `rich_surface_active` tracks a currently drawing cockpit/dialog. The host
/// guard can outlive an individual renderer while still holding raw mode,
/// mouse capture, and the alternate screen across console → launch → capsule.
/// Plain command output is equally corrupting in that gap.
#[must_use]
pub fn rich_terminal_owned() -> bool {
    rich_surface_active() || host_screen_owned()
}

/// Re-enter the host alternate screen after an interactive child returns.
///
/// A baked capsule still drops `?1049l` on detach and returns the terminal to
/// the primary screen; re-asserting the moment the `docker exec` returns means
/// the post-attach work (outcome inspection, the exit outro) renders on the
/// alternate screen instead of flashing the operator's shell. No-op unless a
/// host guard owns the screen.
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
    drop(write!(stderr, "\x1b]0;jackin❯ \u{00b7} {title}\x07"));
    drop(stderr.flush());
}

pub use jackin_core::shorten_home;
