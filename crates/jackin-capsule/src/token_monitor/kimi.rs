//! JSONL reader for Kimi token usage.
//!
//! Reads `~/.kimi/sessions/{GROUP_ID}/{SESSION_UUID}/wire.jsonl`. Each
//! `StatusUpdate` line carries that turn's `token_usage`; totals are recomputed
//! from scratch each poll (re-reading the whole file), so polls never
//! double-count. Cost is filled from the pricing table by the caller (keyed on
//! the `kimi` agent slug when no model is on the wire).

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

#[derive(Default)]
struct Acc {
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    model: Option<String>,
    seen: bool,
}

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_wire_files();
    if files.is_empty() {
        return false;
    }

    let mut acc = Acc::default();
    for path in &files {
        let Some(text) = super::read_file_text(path) else {
            continue;
        };
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            if let Some(model) = val.get("model").and_then(|v| v.as_str()) {
                acc.model = Some(model.to_owned());
            }
            // StatusUpdate messages carry this turn's token_usage.
            if let Some(usage) = val.get("token_usage") {
                acc.input += usage
                    .get("input_other")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                acc.output += usage
                    .get("output")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                acc.cache_read += usage
                    .get("input_cache_read")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                acc.cache_write += usage
                    .get("input_cache_creation")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                acc.seen = true;
            }
        }
    }
    if !acc.seen {
        return false;
    }

    let changed = acc.input != session.totals.input_tokens
        || acc.output != session.totals.output_tokens
        || acc.cache_read != session.totals.cache_read_tokens
        || acc.cache_write != session.totals.cache_write_tokens;
    if changed {
        session.totals.input_tokens = acc.input;
        session.totals.output_tokens = acc.output;
        session.totals.cache_read_tokens = acc.cache_read;
        session.totals.cache_write_tokens = acc.cache_write;
        if acc.model.is_some() {
            session.totals.model = acc.model;
        }
    }
    changed
}

#[cfg(test)]
mod tests;
