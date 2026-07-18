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
use jackin_telemetry::ResultTelemetryExt as _;

fn provider_entries(
    path: impl AsRef<std::path::Path>,
) -> Result<Option<fs::ReadDir>, super::ProviderReadDegraded> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(Some(entries)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => {
            let _error =
                jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
            Err(super::ProviderReadDegraded)
        }
    }
}

fn find_wire_files() -> Result<Vec<PathBuf>, super::ProviderReadDegraded> {
    let mut paths = Vec::new();
    let base = "/home/agent/.kimi/sessions";
    let Some(groups) = provider_entries(base)? else {
        return Ok(paths);
    };
    for group in groups {
        let group = group
            .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
            .map_err(|_| super::ProviderReadDegraded)?;
        let Some(sessions) = provider_entries(group.path())? else {
            continue;
        };
        // No `exists()` stat: a non-existent wire file reads as absent
        // (`read_file_text` maps `NotFound` to `Ok(None)`), so the recompute
        // simply skips it.
        for session in sessions {
            let session = session
                .record_telemetry_error(jackin_telemetry::schema::enums::ErrorType::IoError)
                .map_err(|_| super::ProviderReadDegraded)?;
            paths.push(session.path().join("wire.jsonl"));
        }
    }
    Ok(paths)
}

pub(crate) fn poll_session(session: &mut TokenSession) -> super::PollStatus {
    let Ok(files) = find_wire_files() else {
        return super::PollStatus::Degraded;
    };
    match super::recompute_spend(&files, |text, acc| {
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
    }) {
        Ok(Some(acc)) => super::PollStatus::from_changed(acc.commit(&mut session.totals)),
        Ok(None) => super::PollStatus::Unchanged,
        Err(super::ProviderReadDegraded) => super::PollStatus::Degraded,
    }
}

#[cfg(test)]
mod tests;
