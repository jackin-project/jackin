//! JSONL reader for Claude Code token usage.
//!
//! Reads `/home/agent/.config/claude/projects/**/*.jsonl` (v1.0.30+) or
//! `/home/agent/.claude/projects/**/*.jsonl` (legacy). Each line carries one
//! message's usage; totals are recomputed from scratch each poll (re-reading the
//! whole file), so polls never double-count and need no byte-offset bookkeeping.

use std::path::PathBuf;
use std::time::SystemTime;

use super::TokenSession;

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

    let input_tokens = usage
        .get("input_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let cache_read = usage
        .get("cache_read_input_tokens")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let cost_usd = val.get("costUSD").and_then(serde_json::Value::as_f64);
    let model = msg.get("model").and_then(|v| v.as_str()).map(str::to_owned);
    let is_error = val
        .get("isApiErrorMessage")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let is_sidechain = val
        .get("isSidechain")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    Some(ClaudeUsageLine {
        input_tokens,
        output_tokens,
        cache_creation_input_tokens: cache_creation,
        cache_read_input_tokens: cache_read,
        cost_usd,
        model,
        is_error,
        is_sidechain,
    })
}

fn find_jsonl_files() -> Vec<PathBuf> {
    super::find_provider_files(
        &[
            "/home/agent/.config/claude/projects",
            "/home/agent/.claude/projects",
        ],
        "jsonl",
    )
}

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_jsonl_files();
    if files.is_empty() {
        return false;
    }

    let mut acc = super::SpendAcc::default();
    for path in &files {
        let text = match super::read_file_text(path) {
            Ok(Some(text)) => text,
            Ok(None) => continue,
            // A real read error means we cannot recompute this session's true
            // total; abort and keep the prior totals rather than SET a smaller,
            // partial value (which would silently regress a monotonic counter).
            Err(e) => {
                crate::cdebug!("token monitor: claude read {path:?} failed: {e}");
                return false;
            }
        };
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
    }
    if !acc.seen {
        return false;
    }

    let changed = acc.commit(&mut session.totals);
    if changed && session.totals.window_start.is_none() {
        session.totals.window_start = Some(SystemTime::now());
    }
    changed
}

#[cfg(test)]
mod tests;
