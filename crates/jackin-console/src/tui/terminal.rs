// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Terminal raw-mode lifecycle and teardown helpers.

use crossterm::ExecutableCommand as _;

use crate::ConsoleHostTerminal;

/// 20 Hz: spinner stays fluid and op results surface within ~50ms
/// without hot-spinning. <16ms wastes cycles, >100ms stutters.
pub const TICK_MS: u64 = 50;
pub const MAX_EVENTS_PER_TICK: usize = 256;
pub const MAX_TEARDOWN_DRAIN_EVENTS: usize = 16_384;
pub const TEARDOWN_DRAIN_QUIET_MS: u64 = 30;
pub const TEARDOWN_DRAIN_MAX_MS: u64 = 250;
pub const MOUSE_ESCAPE_GRACE_MS: u64 = 150;

pub fn drain_pending_terminal_events(limit: usize) {
    drain_pending_terminal_events_until_quiet(limit, std::time::Duration::ZERO);
}

pub fn drain_pending_terminal_events_until_quiet(limit: usize, quiet_for: std::time::Duration) {
    let started = std::time::Instant::now();
    for _ in 0..limit {
        let poll_for = if quiet_for.is_zero() {
            std::time::Duration::ZERO
        } else {
            let elapsed = started.elapsed();
            let max = std::time::Duration::from_millis(TEARDOWN_DRAIN_MAX_MS);
            if elapsed >= max {
                break;
            }
            quiet_for.min(max.saturating_sub(elapsed))
        };
        match crossterm::event::poll(poll_for) {
            Ok(true) => {
                drop(crossterm::event::read());
            }
            Ok(false) | Err(_) => break,
        }
    }
}

#[cfg(unix)]
pub fn flush_terminal_input_queue() {
    #[expect(
        clippy::disallowed_methods,
        reason = "terminal setup flush opens /dev/tty briefly outside frame rendering"
    )]
    if let Ok(tty) = std::fs::File::options()
        .read(true)
        .write(true)
        .open("/dev/tty")
    {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "best-effort terminal restore on teardown"
        )]
        let _ = nix::sys::termios::tcflush(&tty, nix::sys::termios::FlushArg::TCIFLUSH);
    }
}

#[cfg(not(unix))]
pub fn flush_terminal_input_queue() {}

pub fn enable_console_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    jackin_tui::terminal_modes::enable_mouse_capture(out)
}

pub fn disable_console_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    jackin_tui::terminal_modes::disable_mouse_capture(out)
}

/// Owns the terminal for an entire launch flow so it never flashes the shell.
///
/// Holds the alternate screen, raw mode, and mouse capture across console →
/// loading cockpit → capsule → exit outro so the terminal never drops back
/// to the cooked primary screen between surfaces.
#[expect(
    missing_debug_implementations,
    reason = "TerminalSession is a raw terminal guard around host callbacks, not diagnostic state."
)]
pub struct TerminalSession {
    host: &'static dyn ConsoleHostTerminal,
}

impl TerminalSession {
    /// Enter raw mode + the alternate screen + mouse capture and mark the
    /// screen owned. The caller holds the returned guard for the whole flow.
    pub fn enter(host: &'static dyn ConsoleHostTerminal) -> std::io::Result<Self> {
        let mut stdout = std::io::stdout();
        crossterm::terminal::enable_raw_mode()?;
        host.begin_debug_buffering();
        let screen = Self { host };
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        enable_console_mouse_capture(&mut stdout)?;
        host.set_host_screen_owned(true);
        Ok(screen)
    }

    /// Returns true while this session owns the host terminal.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.host.host_screen_owned()
    }

    /// Drop to the cooked primary screen for the duration of `f`, then restore
    /// the full-screen session.
    pub fn suspend<T>(&self, f: impl FnOnce() -> T) -> std::io::Result<T> {
        let mut stdout = std::io::stdout();
        drop(disable_console_mouse_capture(&mut stdout));
        crossterm::terminal::disable_raw_mode()?;
        stdout.execute(crossterm::terminal::LeaveAlternateScreen)?;
        stdout.execute(crossterm::cursor::Show)?;
        self.host.set_host_screen_owned(false);
        let out = f();
        crossterm::terminal::enable_raw_mode()?;
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        enable_console_mouse_capture(&mut stdout)?;
        self.host.set_host_screen_owned(true);
        Ok(out)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let mut stdout = std::io::stdout();
        drain_pending_terminal_events_until_quiet(
            MAX_TEARDOWN_DRAIN_EVENTS,
            std::time::Duration::from_millis(TEARDOWN_DRAIN_QUIET_MS),
        );
        drop(disable_console_mouse_capture(&mut stdout));
        drain_pending_terminal_events(MAX_TEARDOWN_DRAIN_EVENTS);
        flush_terminal_input_queue();
        drop(crossterm::terminal::disable_raw_mode());
        drop(stdout.execute(crossterm::terminal::LeaveAlternateScreen));
        drop(stdout.execute(crossterm::cursor::Show));
        self.host.set_host_screen_owned(false);
        self.host.end_debug_buffering();
    }
}

/// Hand the real terminal back to a child process.
pub fn suspend_console_terminal(
    stdout: &mut std::io::Stdout,
    host: &'static dyn ConsoleHostTerminal,
) {
    drop(disable_console_mouse_capture(stdout));
    drop(crossterm::terminal::disable_raw_mode());
    drop(stdout.execute(crossterm::terminal::LeaveAlternateScreen));
    drop(stdout.execute(crossterm::cursor::Show));
    host.end_debug_buffering();
}

/// Re-enter raw-mode + alt-screen after a [`suspend_console_terminal`] detour.
pub fn resume_console_terminal(
    stdout: &mut std::io::Stdout,
    host: &'static dyn ConsoleHostTerminal,
) -> anyhow::Result<()> {
    host.begin_debug_buffering();
    crossterm::terminal::enable_raw_mode()?;
    stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
    enable_console_mouse_capture(stdout)?;
    Ok(())
}
