//! `jackin-xtask` — workspace automation.
//!
//! Invoked via the `cargo xtask` alias (see `.cargo/config.toml`):
//!
//! ```sh
//! # Use cargo
//! cargo xtask construct init-buildx
//! cargo xtask construct build-local
//! cargo xtask construct --help
//! ```
//!
//! The `construct-*` tasks are also exposed as `mise run construct-*` tasks.
//!
//! All task logic is Rust. Subprocesses (`docker`, `git`) are driven via
//! [`std::process::Command`]; the project keeps no shell task scripts. The
//! declarative build graph stays in `docker-bake.hcl`, which this binary
//! invokes rather than reimplementing in flag assembly.

mod construct;
mod docs;
mod pr;
mod pty_fixture;

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
    ///
    /// Use as `cargo xtask construct <subcommand>`.
    #[command(subcommand)]
    Construct(construct::ConstructCommand),
    /// Prepare an isolated local checkout for PR verification.
    ///
    /// Use as `cargo xtask pr prepare <number>`.
    #[command(subcommand)]
    Pr(pr::PrCommand),
    /// Extract a PTY byte-stream fixture from a `--debug` run log for the
    /// capsule render-conformance harness.
    PtyFixture(pty_fixture::PtyFixtureArgs),
    /// Scaffold a new roadmap item and register it in the sidebar.
    ///
    /// Use as `cargo xtask change new <slug> --group <group>`.
    #[command(subcommand)]
    Change(docs::ChangeCommand),
    /// Scaffold or validate research dossiers.
    ///
    /// Use as `cargo xtask research scaffold <slug>` / `research check`.
    #[command(subcommand)]
    Research(docs::ResearchCommand),
    /// Roadmap sidebar maintenance.
    ///
    /// Use as `cargo xtask roadmap audit`.
    #[command(subcommand)]
    Roadmap(docs::RoadmapCommand),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Construct(cmd) => construct::run(cmd),
        Command::Pr(cmd) => pr::run(cmd),
        Command::PtyFixture(args) => pty_fixture::run(args),
        Command::Change(cmd) => docs::run_change(cmd),
        Command::Research(cmd) => docs::run_research(cmd),
        Command::Roadmap(cmd) => docs::run_roadmap(cmd),
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
