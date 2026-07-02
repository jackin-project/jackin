//! JSONL reader for Claude Code token usage.
//!
//! Reads `/home/agent/.config/claude/projects/**/*.jsonl` (v1.0.30+) or
//! `/home/agent/.claude/projects/**/*.jsonl` (legacy). Each line carries one
//! message's usage; totals are recomputed from scratch each poll (re-reading the
//! whole file), so polls never double-count and need no byte-offset bookkeeping.

use std::path::PathBuf;
use std::time::SystemTime;

use super::{TokenSession, json_u64};

/// Per-line token fields from Claude JSONL.
#[derive(Debug, Default)]
struct ClaudeUsageLine {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation_input_tokens: u64,
    cache_read_input_tokens: u64,
    cost_usd: Option<f64>,
    model: Option<String>,
    is_error: bool,
    is_sidechain: bool,
}

fn parse_line(line: &str) -> Option<ClaudeUsageLine> {
    let val: serde_json::Value = serde_json::from_str(line).ok()?;
    let msg = val.get("message")?;
    let usage = msg.get("usage")?;

    Some(ClaudeUsageLine {
        input_tokens: json_u64(usage, "input_tokens"),
        output_tokens: json_u64(usage, "output_tokens"),
        cache_creation_input_tokens: json_u64(usage, "cache_creation_input_tokens"),
        cache_read_input_tokens: json_u64(usage, "cache_read_input_tokens"),
        cost_usd: val.get("costUSD").and_then(serde_json::Value::as_f64),
        model: msg.get("model").and_then(|v| v.as_str()).map(str::to_owned),
        is_error: val
            .get("isApiErrorMessage")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        is_sidechain: val
            .get("isSidechain")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

fn find_jsonl_files() -> Vec<PathBuf> {
    super::find_provider_files(
        &[
            "/home/agent/.config/claude/projects",
            "/home/agent/.claude/projects",
        ],
        "jsonl",
        super::PROVIDER_WALK_DEPTH,
    )
}

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_jsonl_files();
    let Some(acc) = super::recompute_spend(&files, "claude", |text, acc| {
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Some(parsed) = parse_line(line) else {
                continue;
            };
            if parsed.is_sidechain {
                continue; // Skip sidechain replays.
            }
            if parsed.is_error && parsed.input_tokens == 0 && parsed.output_tokens == 0 {
                continue;
            }
            if let Some(m) = parsed.model {
                acc.model = Some(m);
            }
            acc.input += parsed.input_tokens;
            acc.output += parsed.output_tokens;
            acc.cache_read += parsed.cache_read_input_tokens;
            acc.cache_write += parsed.cache_creation_input_tokens;
            if let Some(cost) = parsed.cost_usd {
                acc.cost += cost;
                acc.has_cost = true;
            }
            acc.seen = true;
        }
    }) else {
        return false;
    };

    let changed = acc.commit(&mut session.totals);
    if changed && session.totals.window_start.is_none() {
        session.totals.window_start = Some(SystemTime::now());
    }
    changed
}

#[cfg(test)]
mod tests;
