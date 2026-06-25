//! JSONL reader for Codex token usage.

use std::path::PathBuf;

use super::TokenSession;

fn find_jsonl_files() -> Vec<PathBuf> {
    super::find_provider_files(&["/home/agent/.codex/sessions"], "jsonl")
}

fn parse_raw_usage(obj: &serde_json::Value) -> (u64, u64, u64, u64) {
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
    let reasoning = obj
        .get("reasoning_output_tokens")
        .or_else(|| obj.get("reasoning_tokens"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    (input, output, cached, reasoning)
}

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_jsonl_files();
    if files.is_empty() {
        return false;
    }

    let mut changed = false;

    for path in &files {
        let mut prev_cumulative = (0u64, 0u64, 0u64, 0u64);
        let Some((text, new_offset)) = super::read_new_text(path, &mut session.file_offset) else {
            continue;
        };

        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };

            // Session format: type = "event_msg" with token_count payload
            if val.get("type").and_then(|v| v.as_str()) == Some("event_msg") {
                let Some(payload) = val.get("payload") else {
                    continue;
                };
                if payload.get("type").and_then(|v| v.as_str()) == Some("token_count")
                    && let Some(info) = payload.get("info")
                    && let Some(total) = info.get("total_token_usage")
                {
                    let current = parse_raw_usage(total);
                    // If the cumulative counter is lower than prev (counter
                    // regression or file re-read after seek reset), clamp to 0.
                    let delta = |cur: u64, prev: u64, label: &str| -> u64 {
                        if cur < prev {
                            crate::cdebug!(
                                "token monitor: codex counter regression {} {}<{} in {:?}, clamping to 0",
                                label,
                                cur,
                                prev,
                                path
                            );
                            0
                        } else {
                            cur - prev
                        }
                    };
                    session.totals.input_tokens += delta(current.0, prev_cumulative.0, "input");
                    session.totals.output_tokens += delta(current.1, prev_cumulative.1, "output");
                    session.totals.cache_read_tokens +=
                        delta(current.2, prev_cumulative.2, "cached");
                    prev_cumulative = current;
                    changed = true;
                }
                if let Some(model) = payload.get("model_name").and_then(|v| v.as_str()) {
                    session.totals.model = Some(model.to_owned());
                }
                continue;
            }

            // Headless format: direct usage at top level
            if val.get("usage").is_some() {
                if let Some(usage) = val.get("usage") {
                    let (inp, out, cached, _) = parse_raw_usage(usage);
                    session.totals.input_tokens += inp;
                    session.totals.output_tokens += out;
                    session.totals.cache_read_tokens += cached;
                    changed = true;
                }
                if let Some(cost) = val.get("costUSD").and_then(serde_json::Value::as_f64) {
                    session.totals.cost_usd = Some(session.totals.cost_usd.unwrap_or(0.0) + cost);
                }
            }
        }
        session.file_offset = new_offset;
    }
    changed
}

#[cfg(test)]
mod tests;
