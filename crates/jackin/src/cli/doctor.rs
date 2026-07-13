// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Args;
use owo_colors::OwoColorize;

use crate::cli::BANNER;
use crate::cli::format::OutputFormat;
use crate::preflight::{CheckName, CheckResult, CheckStatus, run_check};
use jackin_core::JackinPaths;

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
        println!("{}", serde_json::to_string_pretty(&envelope)?);
        return Ok(());
    }

    print!("{BANNER}");
    println!("doctor\n");

    let mut any_fail = false;
    for result in &results {
        let status_str = match result.status {
            CheckStatus::Ok => result.status.symbol().green().to_string(),
            CheckStatus::Warn => result.status.symbol().yellow().to_string(),
            CheckStatus::Fail => {
                any_fail = true;
                result.status.symbol().red().bold().to_string()
            }
            CheckStatus::Skip => result.status.symbol().dimmed().to_string(),
        };
        println!("  {}  {}  {}", status_str, result.name, result.message);
        if let Some(hint) = &result.hint {
            println!("         → {}", hint.dimmed());
        }
    }

    println!();
    if any_fail {
        println!(
            "{}  one or more checks failed — see hints above",
            "✗".red().bold()
        );
        println!("  Run with `--debug` if you need a diagnostics run id to share.");
        anyhow::bail!("doctor checks failed");
    }
    println!("{}", "✓  all checks passed (or warned)".green());
    Ok(())
}

async fn gather_check_results(checks: &[CheckName], paths: &JackinPaths) -> Vec<CheckResult> {
    let mut results = Vec::with_capacity(checks.len());
    for &check in checks {
        results.push(run_check(check, paths).await);
    }
    results
}
