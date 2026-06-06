use clap::{CommandFactory, Parser};
use jackin::cli::Cli;
use jackin::cli::dispatch::{self, Action};
use jackin::cli::role::ConsoleArgs;

#[tokio::main(flavor = "current_thread")]
async fn main() {
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
                render_error(&error, debug);
                std::process::exit(1);
            }
        }
        Action::RunCommand(command) => {
            let cli = Cli {
                command: Some(command),
                console_args: ConsoleArgs::default(),
                debug,
            };
            if let Err(error) = Box::pin(jackin::run(cli)).await {
                render_error(&error, debug);
                std::process::exit(1);
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
