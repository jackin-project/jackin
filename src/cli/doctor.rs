use clap::Args;
use owo_colors::OwoColorize;

use crate::cli::format::OutputFormat;
use crate::paths::JackinPaths;
use crate::preflight::{CheckName, CheckStatus, run_check};

/// `jackin doctor` — run pre-flight health checks and print a status table.
#[derive(Debug, Args, PartialEq, Eq)]
#[command(about = "Run health checks for your jackin' setup")]
pub struct DoctorArgs {
    /// Output format (`human` or `json`)
    #[arg(long, value_name = "FORMAT", default_value = "human")]
    pub format: String,
}

impl DoctorArgs {
    pub fn output_format(&self) -> OutputFormat {
        if self.format == "json" {
            OutputFormat::Json
        } else {
            OutputFormat::Human
        }
    }
}

pub async fn run(args: &DoctorArgs, paths: &JackinPaths) -> anyhow::Result<()> {
    let checks = CheckName::all();
    let format = args.output_format();

    if format == OutputFormat::Json {
        return run_json(checks, paths).await;
    }

    println!("jackin' doctor\n");

    let mut any_fail = false;
    for &check in checks {
        let result = run_check(check, paths).await;
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

async fn run_json(checks: &[CheckName], paths: &JackinPaths) -> anyhow::Result<()> {
    let mut results = Vec::new();
    for &check in checks {
        let r = run_check(check, paths).await;
        results.push(serde_json::json!({
            "name": r.name,
            "status": r.status.symbol().trim(),
            "message": r.message,
            "hint": r.hint,
        }));
    }
    let envelope = serde_json::json!({
        "schema_version": "v1",
        "data": results,
    });
    println!("{}", serde_json::to_string_pretty(&envelope)?);
    Ok(())
}
