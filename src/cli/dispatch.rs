//! Top-level dispatch helpers.
//!
//! This module owns the decision logic that drives bare `jackin`, the
//! explicit `jackin console` subcommand, and the deprecated-but-still
//! supported `jackin launch` alias. It lives separately from [`crate::run`]
//! so the decisions are unit-testable without standing up a full
//! [`crate::config::AppConfig`] / [`crate::paths::JackinPaths`].

use std::io::IsTerminal;

use crate::cli::agent::ConsoleArgs;
use crate::cli::{Cli, Command};

/// Minimum columns for the operator console to render usefully.
pub const MIN_TUI_COLS: u16 = 40;

/// Minimum rows for the operator console to render usefully.
pub const MIN_TUI_ROWS: u16 = 15;

/// User-visible deprecation message emitted when `jackin launch` is used.
pub const LAUNCH_DEPRECATION_WARNING: &str = "warning: `jackin launch` is deprecated and will be removed in a future release; use `jackin` or `jackin console` instead";

/// Error message emitted when `jackin console` (or `jackin launch`) is
/// invoked but the current terminal cannot host the TUI.
pub const CONSOLE_REQUIRES_TTY_ERROR: &str =
    "jackin' console requires an interactive terminal (stdout must be a TTY, minimum size 40x15)";

/// What the top-level dispatcher should do after parsing.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Run the operator console.
    RunConsole {
        args: ConsoleArgs,
        /// When true, the operator asked for the console by name (either
        /// `console` or the deprecated `launch`). Non-TUI-capable terminals
        /// must surface an explicit error instead of silently falling back
        /// to `--help`.
        explicit: bool,
        /// When true, the caller reached the console via the deprecated
        /// `jackin launch` alias; the dispatcher must emit the deprecation
        /// warning before handing off to the runner.
        deprecated_alias: bool,
    },
    /// Run a non-console subcommand.
    RunCommand(Command),
    /// Print top-level `--help` and exit 0. This is the silent fallback
    /// chosen for bare `jackin` on a non-interactive stdout.
    PrintHelpAndExit,
    /// Display long-form man page help for a command and exit.
    PrintHelp { command: Vec<String> },
    /// Error: explicit console request on a non-TTY terminal. Carries
    /// `deprecated_alias` so the dispatcher can still emit the `launch`
    /// deprecation warning before the error exit.
    ErrorNotTtyCapable { deprecated_alias: bool },
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
        Some(Command::Launch(args)) => {
            if tui_capable {
                Action::RunConsole {
                    args,
                    explicit: true,
                    deprecated_alias: true,
                }
            } else {
                Action::ErrorNotTtyCapable {
                    deprecated_alias: true,
                }
            }
        }
        Some(Command::Console(args)) => {
            if tui_capable {
                Action::RunConsole {
                    args,
                    explicit: true,
                    deprecated_alias: false,
                }
            } else {
                Action::ErrorNotTtyCapable {
                    deprecated_alias: false,
                }
            }
        }
        Some(Command::Help { command }) => Action::PrintHelp { command },
        Some(other) => Action::RunCommand(other),
        None => {
            if tui_capable {
                Action::RunConsole {
                    args: cli.console_args,
                    explicit: false,
                    deprecated_alias: false,
                }
            } else {
                Action::PrintHelpAndExit
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn size_check_rejects_too_small() {
        assert!(!is_size_tui_capable(39, 24));
        assert!(!is_size_tui_capable(80, 14));
        assert!(!is_size_tui_capable(0, 0));
    }

    #[test]
    fn size_check_accepts_minimum() {
        assert!(is_size_tui_capable(MIN_TUI_COLS, MIN_TUI_ROWS));
    }

    #[test]
    fn size_check_accepts_large() {
        assert!(is_size_tui_capable(200, 80));
    }

    #[test]
    fn bare_jackin_on_tty_runs_console_implicitly() {
        let cli = Cli::try_parse_from(["jackin"]).unwrap();
        let action = classify(cli, true);
        // `debug` matched with `..`: env-backed (`JACKIN_DEBUG`), so its
        // default depends on the runner's env. What this test pins is
        // routing — `bare jackin` on a TTY classifies as implicit
        // RunConsole — not the debug default.
        assert!(matches!(
            action,
            Action::RunConsole {
                args: ConsoleArgs { .. },
                explicit: false,
                deprecated_alias: false,
            }
        ));
    }

    #[test]
    fn bare_jackin_with_top_level_debug_forwards_to_console() {
        let cli = Cli::try_parse_from(["jackin", "--debug"]).unwrap();
        let action = classify(cli, true);
        assert!(matches!(
            action,
            Action::RunConsole {
                args: ConsoleArgs { debug: true },
                explicit: false,
                deprecated_alias: false,
            }
        ));
    }

    #[test]
    fn bare_jackin_without_tty_prints_help_silently() {
        let cli = Cli::try_parse_from(["jackin"]).unwrap();
        let action = classify(cli, false);
        assert_eq!(action, Action::PrintHelpAndExit);
    }

    #[test]
    fn console_subcommand_routes_to_console_runner() {
        let cli = Cli::try_parse_from(["jackin", "console"]).unwrap();
        let action = classify(cli, true);
        // See `bare_jackin_on_tty_runs_console_implicitly` for why
        // `debug` is matched with `..`.
        assert!(matches!(
            action,
            Action::RunConsole {
                args: ConsoleArgs { .. },
                explicit: true,
                deprecated_alias: false,
            }
        ));
    }

    #[test]
    fn console_subcommand_with_debug_routes_explicitly() {
        let cli = Cli::try_parse_from(["jackin", "console", "--debug"]).unwrap();
        let action = classify(cli, true);
        assert!(matches!(
            action,
            Action::RunConsole {
                args: ConsoleArgs { debug: true },
                explicit: true,
                deprecated_alias: false,
            }
        ));
    }

    #[test]
    fn console_subcommand_without_tty_errors() {
        let cli = Cli::try_parse_from(["jackin", "console"]).unwrap();
        let action = classify(cli, false);
        assert_eq!(
            action,
            Action::ErrorNotTtyCapable {
                deprecated_alias: false,
            }
        );
    }

    #[test]
    fn launch_subcommand_still_routes_to_console_runner_with_deprecation_flag() {
        let cli = Cli::try_parse_from(["jackin", "launch"]).unwrap();
        let action = classify(cli, true);
        // See `bare_jackin_on_tty_runs_console_implicitly` for why
        // `debug` is matched with `..`.
        assert!(matches!(
            action,
            Action::RunConsole {
                args: ConsoleArgs { .. },
                explicit: true,
                deprecated_alias: true,
            }
        ));
    }

    #[test]
    fn launch_subcommand_without_tty_errors_with_deprecation_flag() {
        let cli = Cli::try_parse_from(["jackin", "launch"]).unwrap();
        let action = classify(cli, false);
        assert_eq!(
            action,
            Action::ErrorNotTtyCapable {
                deprecated_alias: true,
            }
        );
    }

    #[test]
    fn non_console_subcommand_passes_through() {
        let cli = Cli::try_parse_from(["jackin", "exile"]).unwrap();
        let action = classify(cli, true);
        assert!(matches!(action, Action::RunCommand(Command::Exile)));
    }

    #[test]
    fn non_console_subcommand_passes_through_even_without_tty() {
        // Non-interactive shell scripts must still be able to run
        // subcommands like `jackin exile` without hitting the TTY gate.
        let cli = Cli::try_parse_from(["jackin", "exile"]).unwrap();
        let action = classify(cli, false);
        assert!(matches!(action, Action::RunCommand(Command::Exile)));
    }

    #[test]
    fn deprecation_warning_mentions_deprecated_and_alternatives() {
        assert!(LAUNCH_DEPRECATION_WARNING.contains("deprecated"));
        assert!(LAUNCH_DEPRECATION_WARNING.contains("jackin console"));
        assert!(LAUNCH_DEPRECATION_WARNING.contains("jackin launch"));
    }

    #[test]
    fn console_requires_tty_error_mentions_tty_and_size() {
        assert!(CONSOLE_REQUIRES_TTY_ERROR.contains("TTY"));
        assert!(CONSOLE_REQUIRES_TTY_ERROR.contains("40x15"));
        // The jackin' apostrophe naming rule applies to user-visible strings.
        assert!(CONSOLE_REQUIRES_TTY_ERROR.contains("jackin'"));
    }

    #[test]
    fn help_with_no_args_classifies_to_print_help() {
        let cli = Cli::try_parse_from(["jackin", "help"]).unwrap();
        let action = classify(cli, true);
        assert!(matches!(action, Action::PrintHelp { ref command } if command.is_empty()));
    }

    #[test]
    fn help_with_args_classifies_to_print_help() {
        let cli = Cli::try_parse_from(["jackin", "help", "config", "auth"]).unwrap();
        let action = classify(cli, false);
        assert!(matches!(
            action,
            Action::PrintHelp { ref command } if command == &["config", "auth"]
        ));
    }
}
