//! JSONL reader for Codex token usage.

use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
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
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = obj
        .get("output_tokens")
        .or_else(|| obj.get("completion_tokens"))
        .or_else(|| obj.get("output"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cached = obj
        .get("cached_input_tokens")
        .or_else(|| obj.get("cache_read_input_tokens"))
        .or_else(|| obj.get("cached_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reasoning = obj
        .get("reasoning_output_tokens")
        .or_else(|| obj.get("reasoning_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    (input, output, cached, reasoning)
}

pub fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_jsonl_files();
    if files.is_empty() {
        return false;
    }

    let mut prev_cumulative = (0u64, 0u64, 0u64, 0u64);
    let mut changed = false;

    for path in &files {
        let Ok(mut file) = fs::File::open(path) else { continue };
        if file.seek(SeekFrom::Start(session.file_offset)).is_err() {
            crate::cdebug!("token monitor: seek failed for {:?}, resetting offset", path);
            session.file_offset = 0;
            let _ = file.seek(SeekFrom::Start(0));
        }
        let reader = BufReader::new(&file);
        let mut bytes_read = session.file_offset;

        for line in reader.lines() {
            let Ok(line) = line else { break };
            bytes_read += line.len() as u64 + 1;
            if line.trim().is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) else { continue };

            // Session format: type = "event_msg" with token_count payload
            if val.get("type").and_then(|v| v.as_str()) == Some("event_msg") {
                let payload = match val.get("payload") {
                    Some(p) => p,
                    None => continue,
                };
                if payload.get("type").and_then(|v| v.as_str()) == Some("token_count") {
                    if let Some(info) = payload.get("info") {
                        if let Some(total) = info.get("total_token_usage") {
                            let current = parse_raw_usage(total);
                            let delta = (
                                current.0.saturating_sub(prev_cumulative.0),
                                current.1.saturating_sub(prev_cumulative.1),
                                current.2.saturating_sub(prev_cumulative.2),
                                0u64,
                            );
                            session.totals.input_tokens += delta.0;
                            session.totals.output_tokens += delta.1;
                            session.totals.cache_read_tokens += delta.2;
                            prev_cumulative = current;
                            changed = true;
                        }
                    }
                }
                if let Some(model) = payload.get("model_name").and_then(|v| v.as_str()) {
                    session.totals.model = Some(model.to_string());
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
                if let Some(cost) = val.get("costUSD").and_then(|v| v.as_f64()) {
                    session.totals.cost_usd = Some(session.totals.cost_usd.unwrap_or(0.0) + cost);
                }
            }
        }
        session.file_offset = bytes_read;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_token_reader_computes_per_turn_delta() {
        let line1 = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"output_tokens":50}}}}"#;
        let line2 = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"output_tokens":90}}}}"#;
        let v1: serde_json::Value = serde_json::from_str(line1).unwrap();
        let v2: serde_json::Value = serde_json::from_str(line2).unwrap();
        assert_eq!(v1.get("type").and_then(|v| v.as_str()), Some("event_msg"));
        assert_eq!(v2.get("type").and_then(|v| v.as_str()), Some("event_msg"));
        let info2 = &v2["payload"]["info"]["total_token_usage"];
        let (inp, out, _, _) = parse_raw_usage(info2);
        assert_eq!(inp, 200);
        assert_eq!(out, 90);
    }

    #[test]
    fn codex_token_reader_handles_headless_format() {
        let line = r#"{"usage":{"input_tokens":300,"output_tokens":100},"costUSD":0.15}"#;
        let val: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(val.get("usage").is_some());
        assert_eq!(val.get("costUSD").and_then(|v| v.as_f64()), Some(0.15));
        let (inp, out, _, _) = parse_raw_usage(val.get("usage").unwrap());
        assert_eq!(inp, 300);
        assert_eq!(out, 100);
    }

    #[test]
    fn parse_raw_usage_handles_alternate_field_names() {
        let obj = serde_json::json!({
            "prompt_tokens": 50,
            "completion_tokens": 20,
            "cache_read_input_tokens": 10,
            "reasoning_output_tokens": 5,
        });
        let (inp, out, cached, reasoning) = parse_raw_usage(&obj);
        assert_eq!(inp, 50);
        assert_eq!(out, 20);
        assert_eq!(cached, 10);
        assert_eq!(reasoning, 5);
    }
}
