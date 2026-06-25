//! JSONL reader for Kimi token usage.
//!
//! Reads `~/.kimi/sessions/{GROUP_ID}/{SESSION_UUID}/wire.jsonl`.

use std::fs;
use std::path::PathBuf;

use super::TokenSession;

fn find_wire_files() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let base = "/home/agent/.kimi/sessions";
    let Ok(groups) = fs::read_dir(base) else {
        return paths;
    };
    for group in groups.flatten() {
        let Ok(sessions) = fs::read_dir(group.path()) else {
            continue;
        };
        for session in sessions.flatten() {
            let wire = session.path().join("wire.jsonl");
            if wire.exists() {
                paths.push(wire);
            }
        }
    }
    paths
}

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_wire_files();
    if files.is_empty() {
        return false;
    }
    let mut changed = false;

    for path in &files {
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

            // StatusUpdate messages carry token_usage
            if let Some(usage) = val.get("token_usage") {
                let input = usage
                    .get("input_other")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let output = usage
                    .get("output")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let cache_read = usage
                    .get("input_cache_read")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                let cache_write = usage
                    .get("input_cache_creation")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                session.totals.input_tokens += input;
                session.totals.output_tokens += output;
                session.totals.cache_read_tokens += cache_read;
                session.totals.cache_write_tokens += cache_write;
                changed = true;
            }
        }
        session.file_offset = new_offset;
    }
    changed
}

#[cfg(test)]
mod tests;
