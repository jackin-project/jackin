//! `jackin-xtask` — workspace automation, invoked through mise tasks.
//!
//! All task logic is Rust. Subprocesses (`docker`, `git`) are driven via
//! [`std::process::Command`]; the project keeps no shell task scripts. The
//! declarative build graph stays in `docker-bake.hcl`, which this binary
//! invokes rather than reimplementing in flag assembly.

mod construct;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jackin-xtask", about = "jackin workspace automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Construct base-image build and publish tasks.
    #[command(subcommand)]
    Construct(construct::ConstructCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Construct(cmd) => construct::run(cmd),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            #[expect(
                clippy::print_stderr,
                reason = "xtask is a CLI; the error report is its user-facing output"
            )]
            {
                eprintln!("error: {err:#}");
            }
            ExitCode::FAILURE
        }
    }
}
