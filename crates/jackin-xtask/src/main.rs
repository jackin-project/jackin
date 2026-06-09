//! `jackin-xtask` — workspace automation, invoked through mise tasks.
//!
//! All task logic is Rust. Subprocesses (`docker`, `git`) are driven via
//! [`std::process::Command`]; the project keeps no shell task scripts. The
//! declarative build graph stays in `docker-bake.hcl`, which this binary
//! invokes rather than reimplementing in flag assembly.

mod construct;

use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jackin-xtask", about = "jackin workspace automation tasks")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Construct base-image build and publish tasks.
    #[command(subcommand)]
    Construct(construct::ConstructCommand),

    /// List available tasks (also shown for bare `cargo xtask` with no subcommand).
    List,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Some(Command::Construct(cmd)) => construct::run(cmd),
        Some(Command::List) | None => list_tasks(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            #[expect(
                clippy::print_stderr,
                reason = "jackin-xtask is a CLI; the error report is its user-facing output"
            )]
            {
                eprintln!("error: {err:#}");
            }
            ExitCode::FAILURE
        }
    }
}

fn list_tasks() -> Result<()> {
    println!("Available tasks (primary entrypoint: `cargo xtask`; also exposed via mise):");
    println!();
    println!("# Use cargo");
    println!("cargo xtask construct init-buildx");
    println!("cargo xtask construct doctor-buildx");
    println!("cargo xtask construct reset-buildx");
    println!("cargo xtask construct build-local");
    println!("cargo xtask construct build-platform <amd64|arm64>");
    println!("cargo xtask construct push-platform <amd64|arm64>");
    println!("cargo xtask construct assert-version-unpublished");
    println!("cargo xtask construct publish-manifest");
    println!("cargo xtask construct inspect");
    println!();
    println!("# Or for help");
    println!("cargo xtask list          # (or bare `cargo xtask`)");
    println!("cargo xtask construct --help");
    println!();
    println!("(All construct logic lives in Rust; `docker-bake.hcl` holds the declarative graph.)");
    Ok(())
}
