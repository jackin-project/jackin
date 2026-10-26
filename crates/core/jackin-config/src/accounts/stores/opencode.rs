// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use super::{StoreCandidate, StoreError, StoreKind, read_store_file, select_entry_secret};

const AUTH_JSON_LIMIT: u64 = 1024 * 1024;

pub(crate) fn validate_opencode_auth_layout(data_dir: &Path) -> Result<(), StoreError> {
    let Some(bytes) = read_store_file(&data_dir.join("auth.json"), AUTH_JSON_LIMIT)? else {
        return Ok(());
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Malformed)?;
    let entries = value.as_object().ok_or(StoreError::Malformed)?;
    if entries.is_empty() {
        return Ok(());
    }
    if entries.len() != 1 {
        return Err(StoreError::Unsupported(
            "OpenCode auth.json must contain exactly one provider credential",
        ));
    }
    let Some((provider, entry)) = entries.iter().next() else {
        return Ok(());
    };
    if provider != "opencode-go" {
        return Err(StoreError::Unsupported(
            "OpenCode source-bound profiles currently support only the opencode-go auth entry",
        ));
    }
    if select_entry_secret(entry).is_none() {
        return Err(StoreError::Unsupported(
            "OpenCode auth.json entry has no supported credential",
        ));
    }
    Ok(())
}

pub(crate) fn enumerate_opencode_auth(auth_path: &Path) -> Result<Vec<StoreCandidate>, StoreError> {
    let Some(bytes) = read_store_file(auth_path, AUTH_JSON_LIMIT)? else {
        return Ok(Vec::new());
    };
    parse_opencode_bytes(&bytes, auth_path)
}

fn parse_opencode_bytes(bytes: &[u8], source: &Path) -> Result<Vec<StoreCandidate>, StoreError> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| StoreError::Malformed)?;
    let entries = value.as_object().ok_or(StoreError::Malformed)?;
    let mut providers: Vec<&String> = entries.keys().collect();
    providers.sort();
    let mut candidates = Vec::new();
    for provider in providers {
        let Some(entry) = entries.get(provider) else {
            continue;
        };
        let Some((kind, field, secret)) = select_entry_secret(entry) else {
            continue;
        };
        candidates.push(StoreCandidate::new(
            StoreKind::Opencode,
            provider.as_str().to_owned(),
            None,
            source.to_path_buf(),
            kind,
            field.to_owned(),
            secret.to_owned(),
        ));
    }
    Ok(candidates)
}
