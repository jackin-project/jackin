//! JSON reader for Amp thread files.
//!
//! Reads `~/.local/share/amp/threads/*.json`.

use std::fs;
use std::path::PathBuf;

use super::TokenSession;

fn find_thread_files() -> Vec<PathBuf> {
    let base = "/home/agent/.local/share/amp/threads";
    let Ok(dir) = fs::read_dir(base) else {
        return Vec::new();
    };
    dir.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect()
}

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_thread_files();
    super::recompute_spend(&files, "amp", |content, acc| {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(content) else {
            return;
        };
        // Thread JSON: array of messages, each may have usage metadata.
        let messages: &[serde_json::Value] = match val.as_array() {
            Some(arr) => arr,
            None => match val.get("messages").and_then(|m| m.as_array()) {
                Some(arr) => arr,
                None => return,
            },
        };
        for msg in messages {
            if let Some(usage) = msg.get("usage") {
                let input = usage
                    .get("input_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let output = usage
                    .get("output_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let cache_read = usage
                    .get("cache_read_input_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let cache_write = usage
                    .get("cache_creation_input_tokens")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                acc.input = acc.input.saturating_add(input);
                acc.output = acc.output.saturating_add(output);
                acc.cache_read = acc.cache_read.saturating_add(cache_read);
                acc.cache_write = acc.cache_write.saturating_add(cache_write);
                acc.seen = true;
            }
            if let Some(model) = msg.get("model").and_then(|v| v.as_str()) {
                acc.model = Some(model.to_owned());
            }
        }
    })
    .is_some_and(|acc| acc.commit(&mut session.totals))
}

#[cfg(test)]
mod tests;
