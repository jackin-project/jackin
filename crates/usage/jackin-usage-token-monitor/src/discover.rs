// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_telemetry::{ResultTelemetryExt as _, schema};

use super::{ProviderReadDegraded, SpendAcc};

pub fn json_u64(v: &serde_json::Value, key: &str) -> u64 {
    v.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0)
}

pub const PROVIDER_WALK_DEPTH: usize = 8;

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

pub fn read_file_text(path: &Path) -> std::io::Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

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
