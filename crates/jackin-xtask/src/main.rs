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
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Construct base-image build and publish tasks.
    #[command(subcommand)]
    Construct(construct::ConstructCommand),

    /// List available xtasks (convenient alias: `cargo xtasks`).
    #[command(alias = "list")]
    Xtasks,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Construct(cmd) => construct::run(cmd),
        Command::Xtasks => list_xtasks(),
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

fn list_xtasks() -> Result<()> {
    println!("Available xtasks (run via `cargo xtask <cmd>` or the corresponding `mise` task):");
    println!();
    println!("  construct <subcommand>");
    println!("    init-buildx                 Create and bootstrap the construct Buildx builder");
    println!("    doctor-buildx               Inspect the construct Buildx builder and list builders");
    println!("    reset-buildx                Recreate the construct Buildx builder from scratch");
    println!("    build-local                 Build the construct image for the host platform and load it locally");
    println!("    build-platform <amd64|arm64>  Build for one platform and load locally");
    println!("    push-platform <amd64|arm64>   Push single-platform image by digest (CI)");
    println!("    assert-version-unpublished  Fail if VERSION already published");
    println!("    publish-manifest            Combine digests into multi-platform manifest");
    println!("    inspect                     Print the resolved Bake configuration (dry-run)");
    println!();
    println!("Use `cargo xtask construct --help` for subcommand details.");
    println!("(All construct logic lives in Rust; `docker-bake.hcl` holds the declarative graph.)");
    Ok(())
}
