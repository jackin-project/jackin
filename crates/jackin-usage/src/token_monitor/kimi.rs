// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! JSONL reader for Kimi token usage.
//!
//! Reads `~/.kimi/sessions/{GROUP_ID}/{SESSION_UUID}/wire.jsonl`. Each
//! `StatusUpdate` line carries that turn's `token_usage`; totals are recomputed
//! from scratch each poll (re-reading the whole file), so polls never
//! double-count. Cost is filled from the pricing table by the caller (keyed on
//! the `kimi` agent slug when no model is on the wire).

use std::fs;
use std::path::PathBuf;

use super::{TokenSession, json_u64};

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
        // No `exists()` stat: a non-existent wire file reads as absent
        // (`read_file_text` maps `NotFound` to `Ok(None)`), so the recompute
        // simply skips it.
        for session in sessions.flatten() {
            paths.push(session.path().join("wire.jsonl"));
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
                acc.input += json_u64(usage, "input_other");
                acc.output += json_u64(usage, "output");
                acc.cache_read += json_u64(usage, "input_cache_read");
                acc.cache_write += json_u64(usage, "input_cache_creation");
                acc.seen = true;
            }
        }
    })
    .is_some_and(|acc| acc.commit(&mut session.totals))
}

#[cfg(test)]
mod tests;
