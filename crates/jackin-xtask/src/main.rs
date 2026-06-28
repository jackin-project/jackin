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
mod lint;
mod pr;
mod pty_fixture;
mod schema;

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
    /// Generate pull request body skeletons.
    ///
    /// Use as `cargo xtask pr body`.
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
    /// Use as `cargo xtask roadmap audit` / `roadmap retire <slug>`.
    #[command(subcommand)]
    Roadmap(docs::RoadmapCommand),
    /// Enforce the versioned-schema five-artifact rule on a diff.
    ///
    /// Use as `cargo xtask schema-check --base origin/main`.
    SchemaCheck(schema::SchemaCheckArgs),
    /// File-size ratchet gate (Workstream B of codebase-health-enforcement).
    ///
    /// Use as `cargo xtask lint files` (enforce) or
    /// `cargo xtask lint files --print-budget` (refresh the budget file).
    Lint {
        #[command(subcommand)]
        command: LintCommand,
    },
}

#[derive(Subcommand)]
enum LintCommand {
    /// Enforce the file-size ratchet from `file-size-budget.toml`.
    Files(lint::LintFilesArgs),
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
        Command::SchemaCheck(args) => schema::run(args),
        Command::Lint { command } => match command {
            LintCommand::Files(args) => lint::run(args),
        },
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
