// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Agent registry query command.

use super::request_control;
use anyhow::Result;

use crate::protocol::control::{ClientMsg, ServerMsg};

/// Format for `jackin-capsule agents` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentsFormat {
    Human,
    Json,
}

/// Query the daemon for the agent registry and render it.
///
/// `--format json` emits the registry as a JSON array.
/// Human format renders a table with a `← you` annotation on the caller's row.
/// # Errors
///
/// Returns an error when the daemon request fails, the response is invalid, or
/// JSON output cannot be serialized.
pub async fn run_agents(format: AgentsFormat) -> Result<()> {
    let msg = request_control(&ClientMsg::Agents).await?;
    let records = match msg {
        ServerMsg::AgentRegistry { records } => records,
        ServerMsg::Ack | ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Agents — peer is newer than this CLI"
            )
        }
        other => anyhow::bail!("daemon replied with {} for Agents request", other.kind()),
    };

    // Determine caller's own codename and annotate matching records.
    let my_codename = std::env::var("JACKIN_AGENT_CODENAME").unwrap_or_default();
    let mut records = records;
    if !my_codename.is_empty() {
        for r in &mut records {
            r.is_self = r.codename == my_codename;
        }
    }

    if format == AgentsFormat::Json {
        crate::output::stdout_line(format_args!("{}", serde_json::to_string_pretty(&records)?));
        return Ok(());
    }

    crate::output::stdout_fragment(format_args!("{}", crate::tui::ansi::BRAND_BANNER));
    crate::output::stdout_line(format_args!("agent registry"));
    if let Some(r) = records.iter().find(|r| r.is_self) {
        crate::output::stdout_line(format_args!(
            "\nYou are: {} ({} · {})",
            r.codename,
            r.agent.as_deref().unwrap_or("shell"),
            r.provider.as_deref().unwrap_or("—"),
        ));
    }

    // Split active first, then exited — within each group sort by started_at.
    let mut active: Vec<_> = records.iter().filter(|r| r.status == "active").collect();
    let mut exited: Vec<_> = records.iter().filter(|r| r.status != "active").collect();
    active.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    exited.sort_by(|a, b| a.started_at.cmp(&b.started_at));

    crate::output::stdout_empty_line();
    crate::output::stdout_line(format_args!(
        "  {:<12} {:<10} {:<14} {:<20} {:<20} status",
        "codename", "agent", "provider", "started", "exited"
    ));
    crate::output::stdout_line(format_args!("  {}", "─".repeat(83)));

    for r in active.iter().chain(exited.iter()) {
        let you = if r.is_self { "  ← you" } else { "" };
        crate::output::stdout_line(format_args!(
            "  {:<12} {:<10} {:<14} {:<20} {:<20} {}{}",
            r.codename,
            r.agent.as_deref().unwrap_or("shell"),
            r.provider.as_deref().unwrap_or("—"),
            // Trim the trailing 'Z' and 'T' for compact display
            r.started_at.trim_end_matches('Z').replace('T', " "),
            r.exited_at
                .as_deref()
                .unwrap_or("—")
                .trim_end_matches('Z')
                .replace('T', " "),
            r.status,
            you,
        ));
    }

    Ok(())
}
