//! JSONL reader for Claude Code token usage.
//!
//! Reads `/home/agent/.config/claude/projects/**/*.jsonl` (v1.0.30+)
//! or `/home/agent/.claude/projects/**/*.jsonl` (legacy).

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

/// Find the JSONL file(s) for the current session.
fn find_jsonl_files() -> Vec<PathBuf> {
    super::find_provider_files(
        &[
            "/home/agent/.config/claude/projects",
            "/home/agent/.claude/projects",
        ],
        "jsonl",
    )
}

/// Poll Claude JSONL files for new token data.
/// Returns true when totals changed.
pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_jsonl_files();
    if files.is_empty() {
        return false;
    }

    // Incremental polling: track a single shared byte offset across all files.
    // A production implementation should track per-file offsets via a
    // HashMap<PathBuf, u64>, but a single offset is sufficient for the first
    // implementation where one JSONL file dominates.
    let mut changed = false;
    let mut new_cost: f64 = 0.0;
    let mut has_cost = false;
    let mut new_input: u64 = 0;
    let mut new_output: u64 = 0;
    let mut new_cache_read: u64 = 0;
    let mut new_cache_write: u64 = 0;
    let mut last_model: Option<String> = session.totals.model.clone();

    for path in &files {
        let Some((text, new_offset)) = super::read_new_text(path, &mut session.file_offset) else {
            continue;
        };

        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(parsed) = parse_line(line) {
                if parsed.is_sidechain {
                    continue; // Skip sidechain replays.
                }
                if parsed.is_error && parsed.input_tokens == 0 && parsed.output_tokens == 0 {
                    continue;
                }
                if let Some(ref m) = parsed.model {
                    last_model = Some(m.clone());
                }
                if let Some(cost) = parsed.cost_usd {
                    new_cost += cost;
                    has_cost = true;
                } else {
                    new_input += parsed.input_tokens;
                    new_output += parsed.output_tokens;
                    new_cache_read += parsed.cache_read_input_tokens;
                    new_cache_write += parsed.cache_creation_input_tokens;
                }
                changed = true;
            }
        }
        session.file_offset = new_offset;
    }

    if changed {
        if has_cost {
            session.totals.cost_usd = Some(session.totals.cost_usd.unwrap_or(0.0) + new_cost);
        }
        session.totals.input_tokens += new_input;
        session.totals.output_tokens += new_output;
        session.totals.cache_read_tokens += new_cache_read;
        session.totals.cache_write_tokens += new_cache_write;
        session.totals.model = last_model;
        if session.totals.window_start.is_none() {
            session.totals.window_start = Some(SystemTime::now());
        }
    }
    changed
}

#[cfg(test)]
mod tests;
