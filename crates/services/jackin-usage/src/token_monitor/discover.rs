// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Provider file discovery and spend recompute.

use std::path::Path;

use jackin_telemetry::{ResultTelemetryExt as _, schema};

use super::{ProviderReadDegraded, SpendAcc};

/// Read a `u64` field from a JSON object, defaulting to 0 when the key is
/// absent or not an unsigned integer. Folds the `get(..).and_then(as_u64)
/// .unwrap_or(0)` chain repeated across every provider's usage parse.
pub fn json_u64(v: &serde_json::Value, key: &str) -> u64 {
    v.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0)
}

/// Default recursion bound for providers that nest session logs (Claude one
/// level `projects/<dir>/*.jsonl`, Codex three `sessions/YYYY/MM/DD/*.jsonl`);
/// also a guard against a pathological tree.
pub const PROVIDER_WALK_DEPTH: usize = 8;

/// Walk `base_dirs` up to `max_depth` levels deep and return every file with
/// extension `ext`. `max_depth = 0` reads only the top level of each base dir
/// (Amp's flat `threads/*.json`); deeper providers pass [`PROVIDER_WALK_DEPTH`].
/// A missing or unreadable directory yields no files, never an error.
pub fn find_provider_files(
    base_dirs: &[&str],
    ext: &str,
    max_depth: usize,
) -> Result<Vec<std::path::PathBuf>, ProviderReadDegraded> {
    let mut paths = Vec::new();
    let mut stack: Vec<(std::path::PathBuf, usize)> = base_dirs
        .iter()
        .map(|b| (Path::new(b).to_owned(), 0))
        .collect();
    while let Some((dir, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                let _error = jackin_telemetry::record_error(schema::enums::ErrorType::IoError);
                return Err(ProviderReadDegraded);
            }
        };
        for entry in entries {
            let entry = entry
                .record_telemetry_error(schema::enums::ErrorType::IoError)
                .map_err(|_| ProviderReadDegraded)?;
            let p = entry.path();
            let file_type = entry
                .file_type()
                .record_telemetry_error(schema::enums::ErrorType::IoError)
                .map_err(|_| ProviderReadDegraded)?;
            if file_type.is_dir() {
                if depth < max_depth {
                    stack.push((p, depth + 1));
                }
            } else if p.extension().and_then(|e| e.to_str()) == Some(ext) {
                paths.push(p);
            }
        }
    }
    Ok(paths)
}

/// Read a file in full for a recompute pass. Token logs are re-read whole each
/// poll (adapters recompute totals from scratch), avoiding the per-file
/// byte-offset bookkeeping that silently double-counted across globbed files.
///
/// `Ok(None)` is an absent file (expected — the agent simply has not written it).
/// `Err` is a real IO failure (permission, transient): the caller must NOT treat
/// that as "the file is empty" and recompute a smaller total — under the SET
/// model that would silently regress a monotonic counter. Callers abort the
/// recompute on `Err` and keep the prior totals instead.
pub fn read_file_text(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Recompute a session's spend by reading every provider file whole and folding
/// each file's text into a `SpendAcc` via `fold`. This is the outer shape every
/// adapter shares; only `fold` (the per-file parse) differs.
///
/// An empty successful pass returns `Ok(None)`. A real read failure returns
/// `Err(ProviderReadDegraded)`, allowing the caller to preserve prior totals
/// while reporting the degraded provider probe.
pub fn recompute_spend(
    files: &[std::path::PathBuf],
    mut fold: impl FnMut(&str, &mut SpendAcc),
) -> Result<Option<SpendAcc>, ProviderReadDegraded> {
    let mut acc = SpendAcc::default();
    for path in files {
        match read_file_text(path).record_telemetry_error(schema::enums::ErrorType::IoError) {
            Ok(Some(text)) => fold(&text, &mut acc),
            Ok(None) => {}
            Err(_) => return Err(ProviderReadDegraded),
        }
    }
    Ok(acc.seen.then_some(acc))
}
