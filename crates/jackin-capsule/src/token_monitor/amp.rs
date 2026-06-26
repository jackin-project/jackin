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
    if files.is_empty() {
        return false;
    }

    // Compute totals from scratch (Amp has no per-file byte offset).
    let mut scratch_input: u64 = 0;
    let mut scratch_output: u64 = 0;
    let mut scratch_cache_read: u64 = 0;
    let mut scratch_cache_write: u64 = 0;
    let mut last_model: Option<String> = None;

    for path in &files {
        let content = match super::read_file_text(path) {
            Ok(Some(content)) => content,
            Ok(None) => continue,
            // Abort on a real read error; keep prior totals (see claude.rs).
            Err(e) => {
                crate::cdebug!("token monitor: amp read {path:?} failed: {e}");
                return false;
            }
        };
        let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        // Thread JSON: array of messages, each may have usage metadata
        let messages: &[serde_json::Value] = match val.as_array() {
            Some(arr) => arr,
            None => match val.get("messages").and_then(|m| m.as_array()) {
                Some(arr) => arr,
                None => continue,
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
                scratch_input = scratch_input.saturating_add(input);
                scratch_output = scratch_output.saturating_add(output);
                scratch_cache_read = scratch_cache_read.saturating_add(cache_read);
                scratch_cache_write = scratch_cache_write.saturating_add(cache_write);
            }
            if let Some(model) = msg.get("model").and_then(|v| v.as_str()) {
                last_model = Some(model.to_owned());
            }
        }
    }

    // Only report changed if the totals actually moved.
    let changed = scratch_input != session.totals.input_tokens
        || scratch_output != session.totals.output_tokens
        || scratch_cache_read != session.totals.cache_read_tokens
        || scratch_cache_write != session.totals.cache_write_tokens;

    if changed {
        session.totals.input_tokens = scratch_input;
        session.totals.output_tokens = scratch_output;
        session.totals.cache_read_tokens = scratch_cache_read;
        session.totals.cache_write_tokens = scratch_cache_write;
        // Only update the model when this pass saw one — never clobber a
        // previously-resolved model with `None` (matches the other adapters).
        if last_model.is_some() {
            session.totals.model = last_model;
        }
    }
    changed
}

#[cfg(test)]
mod tests;
