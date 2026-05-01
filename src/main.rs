use clap::{CommandFactory, Parser};
use jackin::cli::Cli;
use jackin::cli::agent::ConsoleArgs;
use jackin::cli::dispatch::{self, Action};

fn main() {
    let cli = Cli::parse();

    match dispatch::classify(cli, dispatch::is_tui_capable()) {
        Action::RunConsole {
            args,
            explicit: _,
            deprecated_alias,
        } => {
            if deprecated_alias {
                eprintln!("{}", dispatch::LAUNCH_DEPRECATION_WARNING);
            }
            let cli = Cli {
                command: Some(jackin::cli::Command::Console(args)),
                console_args: ConsoleArgs::default(),
            };
            if let Err(error) = jackin::run(cli) {
                jackin::tui::fatal(&format!("{error:#}"));
                std::process::exit(1);
            }
        }
        Action::RunCommand(command) => {
            let cli = Cli {
                command: Some(command),
                console_args: ConsoleArgs::default(),
            };
            if let Err(error) = jackin::run(cli) {
                jackin::tui::fatal(&format!("{error:#}"));
                std::process::exit(1);
            }
        }
        Action::PrintHelpAndExit => {
            // Bare `jackin` on a non-interactive stdout: print help
            // silently and exit 0. No warning — this is the expected
            // fallback, not a user error.
            let mut cmd = Cli::command();
            let _ = cmd.print_help();
            println!();
            std::process::exit(0);
        }
        Action::PrintHelp { command } => {
            if let Err(e) = jackin::cli::help::exec(&command) {
                jackin::tui::fatal(&format!("{e:#}"));
                std::process::exit(1);
            }
        }
        Action::ErrorNotTtyCapable { deprecated_alias } => {
            if deprecated_alias {
                eprintln!("{}", dispatch::LAUNCH_DEPRECATION_WARNING);
            }
            eprintln!("error: {}", dispatch::CONSOLE_REQUIRES_TTY_ERROR);
            std::process::exit(1);
        }
    }
}
