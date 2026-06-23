#![expect(
    clippy::print_stderr,
    reason = "standalone role helper renders top-level errors"
)]

use clap::Parser;
use std::process::ExitCode;

use jackin::cli::role::RoleCommand;
use jackin::role_authoring;

/// Validate, migrate, and inspect jackin role repositories.
///
/// Used by CI (jackin-role-action) and role authors to validate the role
/// contract, migrate manifests, and extract metadata from role repositories.
#[derive(Parser)]
#[command(name = "jackin-role", version = env!("JACKIN_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: RoleCommand,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = role_authoring::run(cli.command);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
