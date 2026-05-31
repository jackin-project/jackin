//! Terminal raw-mode lifecycle and teardown helpers.

pub use jackin_console::terminal::TerminalSession;
pub(crate) use jackin_console::terminal::{MAX_EVENTS_PER_TICK, MOUSE_ESCAPE_GRACE_MS, TICK_MS};

struct HostConsoleTerminal;

impl jackin_console::ConsoleHostTerminal for HostConsoleTerminal {
    fn begin_debug_buffering(&self) {
        crate::tui::begin_debug_buffering();
    }

    fn end_debug_buffering(&self) {
        crate::tui::end_debug_buffering();
    }

    fn set_host_screen_owned(&self, owned: bool) {
        crate::tui::set_host_screen_owned(owned);
    }

    fn host_screen_owned(&self) -> bool {
        crate::tui::host_screen_owned()
    }
}

static HOST_CONSOLE_TERMINAL: HostConsoleTerminal = HostConsoleTerminal;

pub(crate) fn host_console_terminal() -> &'static dyn jackin_console::ConsoleHostTerminal {
    &HOST_CONSOLE_TERMINAL
}

/// Hand the real terminal back to a child process: leave raw-mode +
/// alt-screen and stop debug buffering, mirroring `TerminalGuard::drop`
/// minus the input drain (the child reads stdin directly).
pub(crate) fn suspend_console_terminal(stdout: &mut std::io::Stdout) {
    jackin_console::terminal::suspend_console_terminal(stdout, host_console_terminal());
}

/// Re-enter raw-mode + alt-screen after a [`suspend_console_terminal`]
/// detour, mirroring `run_console`'s initial setup so the TUI resumes
/// where it left off.
pub(crate) fn resume_console_terminal(stdout: &mut std::io::Stdout) -> anyhow::Result<()> {
    jackin_console::terminal::resume_console_terminal(stdout, host_console_terminal())
}
