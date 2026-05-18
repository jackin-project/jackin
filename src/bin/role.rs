use clap::{Parser, Subcommand};
use std::process::ExitCode;

use jackin::cli::role::{RoleCommand, RoleRepoPathArgs};
use jackin::role_authoring;

/// Validate, migrate, and inspect jackin role repositories.
///
/// Used by CI (jackin-role-action) and role authors to validate the role
/// contract, migrate manifests, and extract metadata from role repositories.
#[derive(Parser)]
#[command(name = "jackin-role", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a role repository's manifest, Dockerfile, hooks, and env declarations
    Validate(RoleRepoPathArgs),
    /// Migrate a role manifest to the current schema version, then validate it
    Migrate(RoleRepoPathArgs),
    /// Print the construct image version tag pinned in the role Dockerfile
    ConstructVersion(RoleRepoPathArgs),
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Validate(args) => role_authoring::run(RoleCommand::Validate(args)),
        Command::Migrate(args) => role_authoring::run(RoleCommand::Migrate(args)),
        Command::ConstructVersion(args) => role_authoring::run(RoleCommand::ConstructVersion(args)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
