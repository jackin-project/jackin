#![expect(

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

    clippy::print_stderr,
    reason = "standalone role helper renders top-level errors"
)]

use clap::Parser;
use std::process::ExitCode;

use jackin::cli::role::RoleCommand;

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
    let lifecycle = jackin::ProductLifecycle::begin(jackin::BinaryKind::Role);
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _classification =
                jackin::record_parse_outcome(lifecycle, jackin::BinaryKind::Role, &error);
            error.exit();
        }
    };
    let result = run(lifecycle, cli.command);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(lifecycle: jackin::ProductLifecycle, command: RoleCommand) -> anyhow::Result<()> {
    jackin_diagnostics::set_debug_mode(false);
    let paths = jackin_core::JackinPaths::detect()?;
    let command_name = jackin::cli::telemetry::role_command_name(&command);
    let diagnostics = jackin_diagnostics::RunDiagnostics::start(
        &paths,
        false,
        command_name.as_str(),
        jackin_diagnostics::ServiceIdentity::ROLE,
    )?;
    let _diagnostics_guard = diagnostics.activate();
    let invocation = jackin::InvocationTelemetry::start(
        lifecycle,
        command_name,
        jackin_telemetry::schema::enums::AppMode::OneShot,
    );
    let span = invocation.span();
    let result = span.in_scope(|| jackin::role_authoring::run(command));
    diagnostics.emit_run_summary();
    let _classification = invocation.finish(&result);
    result
}
