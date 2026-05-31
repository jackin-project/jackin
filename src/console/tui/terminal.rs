//! Terminal raw-mode lifecycle and teardown helpers.

use crossterm::ExecutableCommand as _;

/// 20 Hz: spinner stays fluid and op results surface within ~50ms
/// without hot-spinning. <16ms wastes cycles, >100ms stutters.
pub(crate) const TICK_MS: u64 = 50;
pub(crate) const MAX_EVENTS_PER_TICK: usize = 256;
pub(crate) const MAX_TEARDOWN_DRAIN_EVENTS: usize = 16_384;
pub(crate) const TEARDOWN_DRAIN_QUIET_MS: u64 = 30;
pub(crate) const TEARDOWN_DRAIN_MAX_MS: u64 = 250;
pub(crate) const MOUSE_ESCAPE_GRACE_MS: u64 = 150;

pub(crate) fn drain_pending_terminal_events(limit: usize) {
    drain_pending_terminal_events_until_quiet(limit, std::time::Duration::ZERO);
}

pub(crate) fn drain_pending_terminal_events_until_quiet(
    limit: usize,
    quiet_for: std::time::Duration,
) {
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
                let _ = crossterm::event::read();
            }
            Ok(false) | Err(_) => break,
        }
    }
}

#[cfg(unix)]
pub(crate) fn flush_terminal_input_queue() {
    if let Ok(tty) = std::fs::File::options()
        .read(true)
        .write(true)
        .open("/dev/tty")
    {
        let _ = nix::sys::termios::tcflush(&tty, nix::sys::termios::FlushArg::TCIFLUSH);
    }
}

#[cfg(not(unix))]
pub(crate) fn flush_terminal_input_queue() {}

pub(crate) fn enable_console_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    // ?1000h press/release, ?1002h drag, ?1003h any-event motion (drives tab
    // hover, matching the in-container multiplexer), ?1015h+?1006h SGR
    // coordinates. ?1003h motion floods only matter across a pty under inertia;
    // host events are local and the manager batches renders at 20Hz, so the
    // cost is paid once per coalesced frame.
    out.write_all(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h")?;
    out.flush()
}

pub(crate) fn disable_console_mouse_capture<W: std::io::Write>(out: &mut W) -> std::io::Result<()> {
    // Disable the exact modes we enable, plus ?1003l defensively in case
    // an older build or another library enabled any-event tracking.
    out.write_all(b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l")?;
    out.flush()
}

/// Owns the terminal for an entire launch flow so it never flashes the shell.
///
/// Holds the alternate screen, raw mode, and mouse capture across console →
/// loading cockpit → capsule → exit outro so the terminal never drops back
/// to the cooked primary screen between surfaces. Each sub-surface checks
/// [`crate::tui::host_screen_owned`] and skips its own enter/leave while this
/// guard is alive; `Drop` restores the terminal exactly once, on every exit
/// path.
pub struct TerminalSession {
    _private: (),
}

impl TerminalSession {
    /// Enter raw mode + the alternate screen + mouse capture and mark the
    /// screen owned. The caller holds the returned guard for the whole flow.
    pub fn enter() -> std::io::Result<Self> {
        let mut stdout = std::io::stdout();
        crossterm::terminal::enable_raw_mode()?;
        crate::tui::begin_debug_buffering();
        let screen = Self { _private: () };
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        enable_console_mouse_capture(&mut stdout)?;
        crate::tui::set_host_screen_owned(true);
        Ok(screen)
    }

    /// Returns true while this session owns the host terminal.
    ///
    /// This is the typed alternative to `crate::tui::host_screen_owned()` for
    /// callers that have a reference to the session. Subsystems without access
    /// (docker, `operator_env`) still fall back to the global flag for now; the
    /// full migration to typed ownership is tracked in Phase 5.
    #[must_use]
    pub fn is_active(&self) -> bool {
        crate::tui::host_screen_owned()
    }

    /// Drop to the cooked primary screen for the duration of `f`, then restore
    /// the full-screen session. Used for the rare interim prompts that sit
    /// between the console and the loading cockpit (sensitive-mount confirm,
    /// agent choice) and expect a normal line-buffered terminal.
    pub fn suspend<T>(&self, f: impl FnOnce() -> T) -> std::io::Result<T> {
        let mut stdout = std::io::stdout();
        let _ = disable_console_mouse_capture(&mut stdout);
        crossterm::terminal::disable_raw_mode()?;
        stdout.execute(crossterm::terminal::LeaveAlternateScreen)?;
        stdout.execute(crossterm::cursor::Show)?;
        crate::tui::set_host_screen_owned(false);
        let out = f();
        crossterm::terminal::enable_raw_mode()?;
        stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
        enable_console_mouse_capture(&mut stdout)?;
        crate::tui::set_host_screen_owned(true);
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
        let _ = disable_console_mouse_capture(&mut stdout);
        drain_pending_terminal_events(MAX_TEARDOWN_DRAIN_EVENTS);
        flush_terminal_input_queue();
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
        let _ = stdout.execute(crossterm::cursor::Show);
        crate::tui::set_host_screen_owned(false);
        crate::tui::end_debug_buffering();
    }
}

/// Hand the real terminal back to a child process: leave raw-mode +
/// alt-screen and stop debug buffering, mirroring `TerminalGuard::drop`
/// minus the input drain (the child reads stdin directly). Paired with
/// [`resume_console_terminal`] around a contained suspend → run → resume.
pub(crate) fn suspend_console_terminal(stdout: &mut std::io::Stdout) {
    let _ = disable_console_mouse_capture(stdout);
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
    let _ = stdout.execute(crossterm::cursor::Show);
    crate::tui::end_debug_buffering();
}

/// Re-enter raw-mode + alt-screen after a [`suspend_console_terminal`]
/// detour, mirroring `run_console`'s initial setup so the TUI resumes
/// where it left off.
pub(crate) fn resume_console_terminal(stdout: &mut std::io::Stdout) -> anyhow::Result<()> {
    crate::tui::begin_debug_buffering();
    crossterm::terminal::enable_raw_mode()?;
    stdout.execute(crossterm::terminal::EnterAlternateScreen)?;
    enable_console_mouse_capture(stdout)?;
    Ok(())
}
