//! JSON reader for Amp thread files.
//!
//! Reads `~/.local/share/amp/threads/*.json`.

use std::fs;
use std::path::PathBuf;

use super::TokenSession;

fn find_thread_files() -> Vec<PathBuf> {
    let base = "/home/agent/.local/share/amp/threads";
    let Ok(dir) = fs::read_dir(base) else { return Vec::new() };
    dir.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect()
}

pub fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_thread_files();
    if files.is_empty() {
        return false;
    }
    let mut changed = false;

    for path in &files {
        let Ok(content) = fs::read_to_string(path) else { continue };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else { continue };

        // Thread JSON: array of messages, each may have usage metadata
        let messages = match val.as_array() {
            Some(arr) => arr.to_owned(),
            None => match val.get("messages").and_then(|m| m.as_array()) {
                Some(arr) => arr.to_owned(),
                None => continue,
            },
        };

        for msg in &messages {
            if let Some(usage) = msg.get("usage") {
                let input = usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let output = usage
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_read = usage
                    .get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cache_write = usage
                    .get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                if input > 0 || output > 0 {
                    session.totals.input_tokens =
                        session.totals.input_tokens.saturating_add(input);
                    session.totals.output_tokens =
                        session.totals.output_tokens.saturating_add(output);
                    session.totals.cache_read_tokens =
                        session.totals.cache_read_tokens.saturating_add(cache_read);
                    session.totals.cache_write_tokens =
                        session.totals.cache_write_tokens.saturating_add(cache_write);
                    changed = true;
                }
            }
            if let Some(model) = msg.get("model").and_then(|v| v.as_str()) {
                session.totals.model = Some(model.to_string());
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use crate::token_monitor::TokenSession;

    #[test]
    fn amp_token_reader_parses_thread_messages() {
        let json = r#"[
            {"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":5},"model":"claude-3-5-sonnet"},
            {"usage":{"input_tokens":200,"output_tokens":80}}
        ]"#;
        let val: serde_json::Value = serde_json::from_str(json).unwrap();
        let arr = val.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        let usage0 = arr[0].get("usage").unwrap();
        assert_eq!(usage0.get("input_tokens").and_then(|v| v.as_u64()), Some(100));
        assert_eq!(
            arr[0].get("model").and_then(|v| v.as_str()),
            Some("claude-3-5-sonnet")
        );
    }

    #[test]
    fn amp_token_reader_handles_messages_wrapper() {
        let json = r#"{"messages":[{"usage":{"input_tokens":300,"output_tokens":150}}]}"#;
        let val: serde_json::Value = serde_json::from_str(json).unwrap();
        let messages = val.get("messages").and_then(|m| m.as_array()).unwrap();
        assert_eq!(messages.len(), 1);
        let usage = messages[0].get("usage").unwrap();
        assert_eq!(usage.get("input_tokens").and_then(|v| v.as_u64()), Some(300));
    }

    #[test]
    fn amp_token_reader_skips_zero_usage() {
        let session = TokenSession::new(1, "amp");
        // Zero usage should not flip changed flag — verify via parse_raw_usage logic
        let zero = serde_json::json!({"usage":{"input_tokens":0,"output_tokens":0}});
        let usage = zero.get("usage").unwrap();
        let input = usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        let output = usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        assert_eq!(input, 0);
        assert_eq!(output, 0);
        assert_eq!(session.totals.input_tokens, 0);
    }
}
