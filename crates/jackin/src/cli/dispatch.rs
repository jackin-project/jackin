//! Top-level dispatch helpers.
//!
//! This module owns the decision logic that drives bare `jackin` and the
//! explicit `jackin console` subcommand. It lives separately from
//! [`crate::run`] so the decisions are unit-testable without standing up a
//! full [`crate::config::AppConfig`] / [`crate::paths::JackinPaths`].

use std::io::IsTerminal;

use crate::cli::role::ConsoleArgs;
use crate::cli::{Cli, Command};

/// Minimum columns for the operator console to render usefully.
pub const MIN_TUI_COLS: u16 = 40;

/// Minimum rows for the operator console to render usefully.
pub const MIN_TUI_ROWS: u16 = 15;

/// Error message emitted when `jackin console` is invoked but the current
/// terminal cannot host the TUI.
pub const CONSOLE_REQUIRES_TTY_ERROR: &str =
    "jackin❯ console requires an interactive terminal (stdout must be a TTY, minimum size 40x15)";

/// What the top-level dispatcher should do after parsing.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Run the operator console.
    RunConsole {
        args: ConsoleArgs,
        /// When true, the operator asked for the console by name (i.e.
        /// `jackin console`). Non-TUI-capable terminals must surface an
        /// explicit error instead of silently falling back to `--help`.
        explicit: bool,
    },
    /// Run a non-console subcommand.
    RunCommand(Command),
    /// Print top-level `--help` and exit 0. This is the silent fallback
    /// chosen for bare `jackin` on a non-interactive stdout.
    PrintHelpAndExit,
    /// Display long-form man page help for a command and exit.
    PrintHelp { command: Vec<String> },
    /// Error: explicit console request on a non-TTY terminal.
    ErrorNotTtyCapable,
}

/// Report whether the current terminal can host the operator console.
///
/// Checks both [`IsTerminal`] on stdout and that the terminal size meets
/// [`MIN_TUI_COLS`] / [`MIN_TUI_ROWS`]. Either check failing returns false.
pub fn is_tui_capable() -> bool {
    if !std::io::stdout().is_terminal() {
        return false;
    }
    match crossterm::terminal::size() {
        Ok((cols, rows)) => is_size_tui_capable(cols, rows),
        Err(_) => false,
    }
}

/// Pure-function form of [`is_tui_capable`]'s size check — easy to test.
#[must_use]
pub const fn is_size_tui_capable(cols: u16, rows: u16) -> bool {
    cols >= MIN_TUI_COLS && rows >= MIN_TUI_ROWS
}

/// Classify a parsed [`Cli`] into the [`Action`] the dispatcher should take.
///
/// `tui_capable` is threaded in rather than queried here so tests can drive
/// both branches without touching the real terminal.
pub fn classify(cli: Cli, tui_capable: bool) -> Action {
    match cli.command {
        Some(Command::Console(args)) => {
            if tui_capable {
                Action::RunConsole {
                    args,
                    explicit: true,
                }
            } else {
                Action::ErrorNotTtyCapable
            }
        }
        Some(Command::Help { command }) => Action::PrintHelp { command },
        Some(other) => Action::RunCommand(other),
        None => {
            if tui_capable {
                Action::RunConsole {
                    args: cli.console_args,
                    explicit: false,
                }
            } else {
                Action::PrintHelpAndExit
            }
        }
    }
}

#[cfg(test)]
mod tests;
