//! JSONL reader for Codex token usage.
//!
//! Reads `~/.codex/sessions/**/*.jsonl` (date-nested rollouts). Recomputes
//! totals from scratch each poll, so re-reads never double-count.

use std::path::PathBuf;

use super::TokenSession;

fn find_jsonl_files() -> Vec<PathBuf> {
    super::find_provider_files(&["/home/agent/.codex/sessions"], "jsonl")
}

fn parse_raw_usage(obj: &serde_json::Value) -> (u64, u64, u64) {
    let input = obj
        .get("input_tokens")
        .or_else(|| obj.get("prompt_tokens"))
        .or_else(|| obj.get("input"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let output = obj
        .get("output_tokens")
        .or_else(|| obj.get("completion_tokens"))
        .or_else(|| obj.get("output"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let cached = obj
        .get("cached_input_tokens")
        .or_else(|| obj.get("cache_read_input_tokens"))
        .or_else(|| obj.get("cached_tokens"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    (input, output, cached)
}

/// Accumulator for one recompute pass over the session files.
#[derive(Default)]
struct Acc {
    /// Latest cumulative `total_token_usage` (session format — monotonic, so we
    /// take the last value, never a sum).
    cumulative: Option<(u64, u64, u64)>,
    /// Summed per-call usage (headless format — each line is one call's usage).
    headless: (u64, u64, u64),
    cost: f64,
    has_cost: bool,
    model: Option<String>,
    seen: bool,
}

fn apply_line(line: &str, acc: &mut Acc) {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };

    // Session format: type = "event_msg" with a cumulative token_count payload.
    if val.get("type").and_then(|v| v.as_str()) == Some("event_msg") {
        let Some(payload) = val.get("payload") else {
            return;
        };
        if payload.get("type").and_then(|v| v.as_str()) == Some("token_count")
            && let Some(info) = payload.get("info")
            && let Some(total) = info.get("total_token_usage")
        {
            acc.cumulative = Some(parse_raw_usage(total));
            acc.seen = true;
        }
        if let Some(model) = payload.get("model_name").and_then(|v| v.as_str()) {
            acc.model = Some(model.to_owned());
        }
        return;
    }

    // Headless format: per-call usage at top level.
    if let Some(usage) = val.get("usage") {
        let (inp, out, cached) = parse_raw_usage(usage);
        acc.headless.0 += inp;
        acc.headless.1 += out;
        acc.headless.2 += cached;
        acc.seen = true;
    }
    if let Some(cost) = val.get("costUSD").and_then(serde_json::Value::as_f64) {
        acc.cost += cost;
        acc.has_cost = true;
    }
}

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_jsonl_files();
    if files.is_empty() {
        return false;
    }

    let mut acc = Acc::default();
    for path in &files {
        let Some(text) = super::read_file_text(path) else {
            continue;
        };
        for line in text.lines() {
            if !line.trim().is_empty() {
                apply_line(line, &mut acc);
            }
        }
    }
    if !acc.seen {
        return false;
    }

    // The cumulative session counter wins when present; otherwise the headless sum.
    let (input, output, cached) = acc.cumulative.unwrap_or(acc.headless);
    let cost = acc.has_cost.then_some(acc.cost);
    let changed = input != session.totals.input_tokens
        || output != session.totals.output_tokens
        || cached != session.totals.cache_read_tokens
        || (cost.is_some() && cost != session.totals.cost_usd);
    if changed {
        session.totals.input_tokens = input;
        session.totals.output_tokens = output;
        session.totals.cache_read_tokens = cached;
        if cost.is_some() {
            session.totals.cost_usd = cost;
        }
        if acc.model.is_some() {
            session.totals.model = acc.model;
        }
    }
    changed
}

#[cfg(test)]
mod tests;
