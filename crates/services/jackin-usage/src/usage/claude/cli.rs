// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Claude` CLI usage parsing and diagnostics.

use super::{
    CLAUDE_SESSION_WINDOW_SECONDS, CLAUDE_WEEKLY_WINDOW_SECONDS, ClaudeQuotaWindow,
    parse_claude_usage_output, run_claude_usage_diagnostic,
};
use jackin_protocol::control::{QuotaBucketView, StatusSlot};
use jackin_usage_provider_core::ProviderError;
use jackin_usage_provider_core::{
    CLAUDE_CODE_USER_AGENT_FALLBACK, CLAUDE_VERSION_TIMEOUT, CliOutput, run_cli_with_timeout_full,
};
use serde::Deserialize;
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub(crate) struct ClaudeCliUsage {
    pub(crate) session_used: Option<f64>,
    pub(crate) weekly_used: Option<f64>,
    pub(crate) sonnet_used: Option<f64>,
    /// Per-model weekly windows the CLI prints as `Current week (<model>): …`
    /// (Fable today; future model codenames). Each entry is `(model label,
    /// percent used)`. Distinct from `sonnet_used`, which preserves the legacy
    /// `(Sonnet only)` line and its "Sonnet" bucket label.
    pub(crate) scoped_weekly: Vec<(String, f64)>,
}

impl ClaudeCliUsage {
    pub(crate) fn buckets(&self) -> Vec<QuotaBucketView> {
        // The CLI fallback reuses the same unified window model + builder as
        // the OAuth path, so a CLI "Weekly" line and an OAuth `weekly_all`
        // limit render identically (headline slot, over-cap label). CLI windows
        // carry no timestamps, so `now` is unused for pace/reset formatting.
        let mut windows: Vec<ClaudeQuotaWindow> = Vec::new();
        if let Some(used) = self.session_used {
            windows.push(ClaudeQuotaWindow::headline(
                "Session",
                StatusSlot::Session,
                used,
                Some(CLAUDE_SESSION_WINDOW_SECONDS),
            ));
        }
        if let Some(used) = self.weekly_used {
            windows.push(ClaudeQuotaWindow::headline(
                "Weekly",
                StatusSlot::Weekly,
                used,
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ));
        }
        if let Some(used) = self.sonnet_used {
            windows.push(ClaudeQuotaWindow::scoped("Sonnet", used));
        }
        for (label, used) in &self.scoped_weekly {
            windows.push(ClaudeQuotaWindow::scoped(label, *used));
        }
        windows.into_iter().map(|w| w.into_bucket(0)).collect()
    }
}

pub(crate) fn claude_code_user_agent() -> String {
    // The Claude Code version is stable for the process lifetime, so resolve the
    // UA once instead of spawning `claude --version` on every usage fetch — that
    // per-probe subprocess was a measurable slice of the load latency (Bug 3).
    static CACHED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CACHED
        .get_or_init(|| {
            claude_code_user_agent_with(|command, args, timeout| {
                run_cli_with_timeout_full(command, args, timeout)
            })
            .unwrap_or_else(|| CLAUDE_CODE_USER_AGENT_FALLBACK.to_owned())
        })
        .clone()
}

pub(crate) fn claude_code_user_agent_with<F>(mut runner: F) -> Option<String>
where
    F: FnMut(&str, &[&str], Duration) -> Result<CliOutput, String>,
{
    let output = runner("claude", &["--version"], CLAUDE_VERSION_TIMEOUT).ok()?;
    if !output.success {
        return None;
    }
    let text = format!("{}\n{}", output.stdout, output.stderr);
    claude_code_version_from_text(&text).map(|version| format!("claude-code/{version}"))
}

pub(crate) fn claude_code_version_from_text(text: &str) -> Option<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '.' || ch == '-'))
        .find(|part| {
            let mut segments = part.split('.');
            matches!(
                (segments.next(), segments.next(), segments.next()),
                (Some(major), Some(minor), Some(patch))
                    if major.chars().all(|ch| ch.is_ascii_digit())
                        && minor.chars().all(|ch| ch.is_ascii_digit())
                        && patch.chars().all(|ch| ch.is_ascii_digit())
            )
        })
        .map(str::to_owned)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeUsageDiagnostic {
    pub command: String,
    pub args: Vec<String>,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub fetched_at_epoch: i64,
}

pub(crate) fn fetch_claude_cli_usage() -> Result<ClaudeCliUsage, ProviderError> {
    let diagnostic = run_claude_usage_diagnostic().map_err(ProviderError::from)?;
    if !diagnostic.success {
        return Err(ProviderError::from(format!(
            "Claude CLI usage exited with status {:?}",
            diagnostic.exit_code
        )));
    }
    parse_claude_usage_output(&diagnostic.stdout)
        .ok_or_else(|| ProviderError::from("Claude CLI usage output was not recognized".to_owned()))
}
