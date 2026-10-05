// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Args;
use owo_colors::OwoColorize;
use std::io::Write;

use crate::cli::BANNER;
use crate::cli::format::OutputFormat;
use crate::preflight::{CheckName, CheckResult, CheckStatus, run_check};
use jackin_core::JackinPaths;

#[cfg(test)]
mod tests;

/// `jackin doctor` — run pre-flight health checks and print a status table.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(about = "Run health checks for your jackin❯ setup")]
pub struct DoctorArgs {
    /// Output format (`human` or `json`)
    #[arg(long, value_name = "FORMAT", default_value = "human")]
    pub format: String,
}

pub async fn run(args: &DoctorArgs, paths: &JackinPaths) -> anyhow::Result<()> {
    let format = OutputFormat::parse(&args.format);
    let results = gather_check_results(CheckName::all(), paths).await;
    report_results(format, &results, &mut std::io::stdout().lock())
}

fn report_results(
    format: OutputFormat,
    results: &[CheckResult],
    output: &mut impl Write,
) -> anyhow::Result<()> {
    let any_fail = results
        .iter()
        .any(|result| result.status == CheckStatus::Fail);

    if format == OutputFormat::Json {
        let json_rows: Vec<_> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name,
                    "status": r.status.symbol().trim(),
                    "message": r.message,
                    "hint": r.hint,
                })
            })
            .collect();
        let envelope = serde_json::json!({ "schema_version": "v1", "data": json_rows });
        writeln!(output, "{}", serde_json::to_string_pretty(&envelope)?)?;
    } else {
        write!(output, "{BANNER}")?;
        writeln!(output, "doctor\n")?;

        for result in results {
            let status_str = match result.status {
                CheckStatus::Ok => result.status.symbol().green().to_string(),
                CheckStatus::Warn => result.status.symbol().yellow().to_string(),
                CheckStatus::Fail => result.status.symbol().red().bold().to_string(),
                CheckStatus::Skip => result.status.symbol().dimmed().to_string(),
            };
            writeln!(output, "  {}  {}  {}", status_str, result.name, result.message)?;
            if let Some(hint) = &result.hint {
                writeln!(output, "         → {}", hint.dimmed())?;
            }
        }

        writeln!(output)?;
        if any_fail {
            writeln!(
                output,
                "{}  one or more checks failed — see hints above",
                "✗".red().bold()
            )?;
            writeln!(
                output,
                "  Run with `--debug` for additional operator output; share the invocation ID when OTLP export is configured."
            )?;
        } else {
            writeln!(output, "{}", "✓  all checks passed (or warned)".green())?;
        }
    }

    if any_fail {
        anyhow::bail!("doctor checks failed");
    }
    Ok(())
}

async fn gather_check_results(checks: &[CheckName], paths: &JackinPaths) -> Vec<CheckResult> {
    let mut results = Vec::with_capacity(checks.len());
    for &check in checks {
        results.push(run_check(check, paths).await);
    }
    results
}
