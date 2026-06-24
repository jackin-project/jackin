//! Tests for `dispatch`.
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
        }
    ));
}

#[test]
fn bare_jackin_with_top_level_debug_forwards_to_console() {
    let cli = Cli::try_parse_from(["jackin", "--debug"]).unwrap();
    assert!(cli.debug, "--debug must be set on the global Cli flag");
    let action = classify(cli, true);
    assert!(matches!(
        action,
        Action::RunConsole {
            args: ConsoleArgs { .. },
            explicit: false,
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
        }
    ));
}

#[test]
fn console_subcommand_with_debug_routes_explicitly() {
    let cli = Cli::try_parse_from(["jackin", "console", "--debug"]).unwrap();
    assert!(cli.debug, "--debug must be set on the global Cli flag");
    let action = classify(cli, true);
    assert!(matches!(
        action,
        Action::RunConsole {
            args: ConsoleArgs { .. },
            explicit: true,
        }
    ));
}

#[test]
fn console_subcommand_without_tty_errors() {
    let cli = Cli::try_parse_from(["jackin", "console"]).unwrap();
    let action = classify(cli, false);
    assert_eq!(action, Action::ErrorNotTtyCapable);
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
fn console_requires_tty_error_mentions_tty_and_size() {
    assert!(CONSOLE_REQUIRES_TTY_ERROR.contains("TTY"));
    assert!(CONSOLE_REQUIRES_TTY_ERROR.contains("40x15"));
    // The jackin❯ brand spelling rule applies to user-visible strings.
    assert!(CONSOLE_REQUIRES_TTY_ERROR.contains("jackin❯"));
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
