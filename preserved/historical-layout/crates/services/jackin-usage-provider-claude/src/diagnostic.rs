// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Claude CLI usage diagnostics.

use std::time::Duration;

use jackin_usage_provider_core::{
    CliOutput, PROVIDER_CLI_TIMEOUT, now_epoch, percent_before_used, run_cli_with_timeout_full,
};

use super::{ClaudeCliUsage, ClaudeUsageDiagnostic};

pub fn run_claude_usage_diagnostic() -> Result<ClaudeUsageDiagnostic, String> {
    run_claude_usage_diagnostic_with(|command, args, timeout| {
        run_cli_with_timeout_full(command, args, timeout)
    })
}

pub fn run_claude_usage_diagnostic_with<F>(mut runner: F) -> Result<ClaudeUsageDiagnostic, String>
where
    F: FnMut(&str, &[&str], Duration) -> Result<CliOutput, String>,
{
    let args = ["-p", "/usage"];
    let output = runner("claude", &args, PROVIDER_CLI_TIMEOUT)?;
    Ok(ClaudeUsageDiagnostic {
        command: "claude".to_owned(),
        args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        success: output.success,
        exit_code: output.exit_code,
        stdout: output.stdout,
        stderr: output.stderr,
        fetched_at_epoch: now_epoch(),
    })
}

pub fn parse_claude_usage_output(text: &str) -> Option<ClaudeCliUsage> {
    let mut usage = ClaudeCliUsage::default();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("Current session:") {
            usage.session_used = percent_before_used(line);
        } else if line.starts_with("Current week (all models):") {
            usage.weekly_used = percent_before_used(line);
        } else if line.starts_with("Current week (Sonnet only):") {
            usage.sonnet_used = percent_before_used(line);
        } else if let Some(rest) = line.strip_prefix("Current week (") {
            // Per-model weekly line, e.g. "Current week (Fable): 35% used · …".
            // The model name is the text between the parens; "all models" and
            // "Sonnet only" are handled by the explicit branches above, so
            // anything reaching here is a model-scoped window (Fable today,
            // future codenames tomorrow). Surfaced generically so a new model
            // prints without a per-model parser edit.
            if let Some(close) = rest.find(')') {
                let label = rest[..close].trim();
                if !label.is_empty()
                    && let Some(percent) = percent_before_used(line)
                {
                    usage.scoped_weekly.push((label.to_owned(), percent));
                }
            }
        }
    }
    (usage.session_used.is_some()
        || usage.weekly_used.is_some()
        || usage.sonnet_used.is_some()
        || !usage.scoped_weekly.is_empty())
    .then_some(usage)
}
