#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "binary entrypoint renders clap help and top-level errors"
)]

use clap::{CommandFactory, Parser};
use jackin::cli::Cli;
use jackin::cli::dispatch::{self, Action};
use jackin::cli::role::ConsoleArgs;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    jackin::install_default_tls_provider();

    let cli = Cli::parse();
    let debug = cli.debug;

    match dispatch::classify(cli, dispatch::is_tui_capable()) {
        Action::RunConsole { args, explicit: _ } => {
            let cli = Cli {
                command: Some(jackin::cli::Command::Console(args)),
                console_args: ConsoleArgs::default(),
                debug,
            };
            if let Err(error) = Box::pin(jackin::run(cli)).await {
                exit_for_run_error(&error, debug);
            }
        }
        Action::RunCommand(command) => {
            let cli = Cli {
                command: Some(command),
                console_args: ConsoleArgs::default(),
                debug,
            };
            if let Err(error) = Box::pin(jackin::run(cli)).await {
                exit_for_run_error(&error, debug);
            }
        }
        Action::PrintHelpAndExit => {
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
    if jackin::runtime::progress::LaunchCancelled::is_cancel(error) {
        std::process::exit(0);
    }
    render_error(error, debug);
    std::process::exit(1);
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
        jackin::tui::fatal(&format!("{error:#}"));
    }
}
