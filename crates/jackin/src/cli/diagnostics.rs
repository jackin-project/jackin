// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! OTLP delivery validation command.

use clap::Subcommand;

#[derive(Debug, Subcommand, PartialEq, Eq)]
pub enum DiagnosticsCommand {
    /// Emit one marked log, trace, and metric and confirm OTLP delivery
    Validate,
}

pub fn run(command: &DiagnosticsCommand) -> anyhow::Result<()> {
    match command {
        DiagnosticsCommand::Validate => validate(),
    }
}

fn validate() -> anyhow::Result<()> {
    let report = jackin_diagnostics::validate_delivery()?;
    let endpoint = jackin_diagnostics::configured_endpoint_summary()
        .unwrap_or_else(|| "configured endpoint".to_owned());
    println!("telemetry: endpoint {endpoint}");
    println!("signals:   traces ok  logs ok  metrics ok");
    println!(
        "delivery:  confirmed (flush {}ms)",
        report.elapsed.as_millis()
    );
    Ok(())
}
