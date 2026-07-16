//! jackin: host CLI binary and top-level dispatch.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`main`] — host CLI binary entry.

#![expect(

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

    clippy::print_stdout,
    clippy::print_stderr,
    reason = "binary entrypoint renders clap help and top-level errors"
)]

use std::io::IsTerminal;

use clap::{CommandFactory, Parser};
use jackin::cli::Cli;
use jackin::cli::dispatch::{self, Action};
use jackin::cli::role::ConsoleArgs;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let lifecycle = jackin::ProductLifecycle::begin(jackin::BinaryKind::Host);
    jackin::install_default_tls_provider();

    // `try_parse` instead of `parse` so we can render the frozen-rain banner
    // for the root `--help`/`--version` ourselves before clap prints the help
    // body — clap reflows multi-line ANSI art passed through `before_help`.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => handle_parse_error(err, lifecycle),
    };
    let debug = cli.debug;

    match dispatch::classify(cli, dispatch::is_tui_capable()) {
        Action::RunConsole { args, explicit: _ } => {
            let cli = Cli {
                command: Some(jackin::cli::Command::Console(args)),
                console_args: ConsoleArgs::default(),
                debug,
            };
            if let Err(error) = Box::pin(jackin::run(cli, lifecycle)).await {
                exit_for_run_error(&error, debug);
            }
        }
        Action::RunCommand(command) => {
            let cli = Cli {
                command: Some(command),
                console_args: ConsoleArgs::default(),
                debug,
            };
            if let Err(error) = Box::pin(jackin::run(cli, lifecycle)).await {
                exit_for_run_error(&error, debug);
            }
        }
        Action::PrintHelpAndExit => {
            // Bare `jackin` on a non-interactive stdout: one-line pill (the
            // root command no longer carries clap's `before_help`), then help.
            print!("{}", jackin::brand_output::BRAND_BANNER);
            let mut cmd = Cli::command();
            drop(cmd.print_help());
            println!();
            std::process::exit(0);
        }
        Action::PrintHelp { command } => {
            if let Err(e) = jackin::cli::help::exec(&command) {
                render_error(&e, debug);
                std::process::exit(1);
            }
        }
        Action::ErrorNotTtyCapable => {
            eprintln!("error: {}", dispatch::CONSOLE_REQUIRES_TTY_ERROR);
            std::process::exit(1);
        }
    }
}

/// Handle a clap parse outcome that is not a successful `Cli`.
///
/// `--help`/`--version` surface as `Err(DisplayHelp|DisplayVersion)`. For the
/// root `--help` we print the frozen-rain banner (or the one-line pill on a
/// narrow/piped terminal) before clap renders the help body; subcommand help
/// keeps clap's own `before_help` pill. Real usage errors fall through to
/// clap's default rendering (stderr, exit 2).
#[expect(
    clippy::exit,
    reason = "binary entrypoint — exit after rendering the version splash"
)]
fn handle_parse_error(err: clap::Error, _lifecycle: jackin::ProductLifecycle) -> ! {
    use clap::error::ErrorKind;
    let classification = jackin::classify_parse_error(&err);
    debug_assert_eq!(classification.exit_code, i64::from(err.exit_code()));
    match err.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            print_root_help_banner();
        }
        ErrorKind::DisplayVersion if std::io::stdout().is_terminal() => {
            // Interactive `--version`: brand splash instead of clap's plain
            // `jackin <version>` line. Piped output falls through to clap.
            let version = Cli::command().get_version().unwrap_or_default().to_owned();
            print!("{}", jackin::brand_output::version_splash(&version));
            std::process::exit(0);
        }
        _ => {}
    }
    err.exit();
}

/// Print the brand banner above the root `jackin --help` output.
///
/// No-op for subcommand help (clap's `before_help` pill covers those). On an
/// interactive terminal wide enough for the art, prints the frozen-rain
/// banner; otherwise the one-line pill, which survives piping and narrow
/// widths.
fn print_root_help_banner() {
    let cmd = Cli::command();
    let is_subcommand_help = {
        let names: Vec<String> = cmd
            .get_subcommands()
            .map(|sub| sub.get_name().to_owned())
            .collect();
        std::env::args().skip(1).any(|arg| names.contains(&arg))
    };
    if is_subcommand_help {
        return; // clap's `before_help` pill covers subcommand help
    }
    // The root command no longer carries clap's pill, so the binary always
    // prints a brand mark here: the big frozen-rain banner with the centered
    // lockup on a roomy interactive terminal, otherwise the one-line pill.
    let interactive = std::io::stdout().is_terminal();
    match crossterm::terminal::size() {
        Ok((cols, rows))
            if interactive && cols >= jackin::brand_output::HELP_BANNER_MIN_COLS && rows >= 20 =>
        {
            print!("{}", jackin::brand_output::help_banner(cols));
        }
        _ => print!("{}", jackin::brand_output::BRAND_BANNER),
    }
}

/// Terminate after a failed `jackin::run`, distinguishing a deliberate
/// operator cancel from a real failure.
///
/// Operator cancel (Ctrl+C / Ctrl+Q / a Cancel modal) is an intent, not an
/// error: exit cleanly with status 0 and render nothing — the launch surface
/// has already restored the terminal. Every other error renders and exits 1.
#[expect(
    clippy::exit,
    reason = "binary entrypoint — exit is the correct mechanism"
)]
fn exit_for_run_error(error: &anyhow::Error, debug: bool) -> ! {
    let classification = jackin::classify_error(error);
    if classification.failed() {
        render_error(error, debug);
    }
    std::process::exit(i32::try_from(classification.exit_code).unwrap_or(1));
}

/// Render an error at the binary entry point.
///
/// Downcasts to `JackinError` for a structured friendly block; falls back to
/// the existing `{error:#}` chain rendering for unrecognized errors.
fn render_error(error: &anyhow::Error, debug: bool) {
    use owo_colors::OwoColorize;
    if let Some(jackin_err) = error.downcast_ref::<jackin::error::JackinError>() {
        jackin_err.user_message().render();
        if debug {
            eprintln!();
            eprintln!("  {} {error:#}", "detail:".dimmed());
        }
    } else {
        jackin_launch::output::fatal(&format!("{error:#}"));
    }
}
