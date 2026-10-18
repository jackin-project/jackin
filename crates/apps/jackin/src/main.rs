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
                command: Some(*command),
                console_args: ConsoleArgs::default(),
                debug,
            };
            if let Err(error) = Box::pin(jackin::run(cli, lifecycle)).await {
                exit_for_run_error(&error, debug);
            }
        }
        Action::PrintHelpAndExit => {
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

#[expect(
    clippy::exit,
    reason = "binary entrypoint — exit after rendering the version splash"
)]
fn handle_parse_error(err: clap::Error, lifecycle: jackin::ProductLifecycle) -> ! {
    use clap::error::ErrorKind;
    let classification = jackin::record_parse_outcome(lifecycle, jackin::BinaryKind::Host, &err);
    debug_assert_eq!(classification.exit_code, i64::from(err.exit_code()));
    match err.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            print_root_help_banner();
        }
        ErrorKind::DisplayVersion if std::io::stdout().is_terminal() => {
            let version = Cli::command().get_version().unwrap_or_default().to_owned();
            print!("{}", jackin::brand_output::version_splash(&version));
            std::process::exit(0);
        }
        _ => {}
    }
    err.exit();
}

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
        return;
    }
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

#[expect(
    clippy::exit,
    reason = "binary entrypoint — exit is the correct mechanism"
)]
fn exit_for_run_error(error: &anyhow::Error, debug: bool) -> ! {
    if let Some((exit_code, json)) = usage_cli_error(error) {
        if !json.is_empty() {
            println!("{json}");
        }
        std::process::exit(exit_code);
    }
    let classification = jackin::classify_error(error);
    if classification.failed() {
        render_error(error, debug);
    }
    std::process::exit(i32::try_from(classification.exit_code).unwrap_or(1));
}

fn usage_cli_error(error: &anyhow::Error) -> Option<(i32, &str)> {
    if let Some(error) = error.downcast_ref::<jackin::cli::usage::UsageCommandExit>() {
        return Some((error.exit_code(), error.json()));
    }
    error
        .downcast_ref::<jackin::cli::workflow::WorkflowCommandExit>()
        .map(|error| (error.exit_code(), error.json()))
}

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

#[cfg(test)]
mod tests;
