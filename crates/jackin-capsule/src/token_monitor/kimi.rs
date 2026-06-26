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

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    let files = find_wire_files();
    super::recompute_spend(&files, "kimi", |text, acc| {
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
    })
    .is_some_and(|acc| acc.commit(&mut session.totals))
}

#[cfg(test)]
mod tests;
