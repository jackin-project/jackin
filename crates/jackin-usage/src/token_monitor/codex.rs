// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! JSONL reader for Codex token usage.
//!
//! Reads `~/.codex/sessions/**/*.jsonl` (date-nested rollouts). Recomputes
//! totals from scratch each poll, so re-reads never double-count.

use std::path::PathBuf;

use super::TokenSession;

fn find_jsonl_files() -> Vec<PathBuf> {
    super::find_provider_files(
        &["/home/agent/.codex/sessions"],
        "jsonl",
        super::PROVIDER_WALK_DEPTH,
    )
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

pub(crate) fn poll_session(session: &mut TokenSession) -> super::PollStatus {
    let files = find_jsonl_files();
    // Aggregate ACROSS files (one rollout per session; codex retains several).
    // Each session file is monotonic-cumulative XOR headless-per-call, so take
    // that file's own total — last cumulative, or its headless sum — and add it
    // to the running aggregate. A single `Acc` across all files would instead let
    // the last-walked file's cumulative overwrite the rest. Codex carries no
    // cache-write dimension, so `SpendAcc.cache_write` stays 0.
    match super::recompute_spend(&files, |text, total| {
        let mut acc = Acc::default();
        for line in text.lines() {
            if !line.trim().is_empty() {
                apply_line(line, &mut acc);
            }
        }
        if !acc.seen {
            return;
        }
        total.seen = true;
        // The file's cumulative counter wins when present; otherwise its headless sum.
        let (i, o, c) = acc.cumulative.unwrap_or(acc.headless);
        total.input += i;
        total.output += o;
        total.cache_read += c;
        if acc.has_cost {
            total.cost += acc.cost;
            total.has_cost = true;
        }
        if acc.model.is_some() {
            total.model = acc.model;
        }
    }) {
        Ok(Some(total)) => super::PollStatus::from_changed(total.commit(&mut session.totals)),
        Ok(None) => super::PollStatus::Unchanged,
        Err(super::ProviderReadDegraded) => super::PollStatus::Degraded,
    }
}

#[cfg(test)]
mod tests;
