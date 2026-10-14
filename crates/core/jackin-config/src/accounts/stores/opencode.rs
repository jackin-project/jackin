// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

#[cfg(test)]
use super::CredentialKind;
#[cfg(test)]
use super::sqlite::{
    SqlValue, SqliteImage, TableRow, find_column, find_table_schema, parse_column_names,
    text_value, wal_sibling_path,
};
use super::{StoreCandidate, StoreError, StoreKind, read_store_file, select_entry_secret};

const AUTH_JSON_LIMIT: u64 = 1024 * 1024;
#[cfg(test)]
const DB_LIMIT: u64 = 8 * 1024 * 1024;
#[cfg(test)]
const WAL_LIMIT: u64 = 8 * 1024 * 1024;

#[cfg(test)]
const VALUE_COLUMNS: [&str; 1] = ["value"];
#[cfg(test)]
const PROVIDER_COLUMNS: [&str; 3] = ["integration_id", "connector_id", "label"];
#[cfg(test)]
const METHOD_COLUMNS: [&str; 2] = ["method_id", "method"];
#[cfg(test)]
const ACTIVE_COLUMNS: [&str; 1] = ["active"];

#[cfg(test)]
pub(crate) fn enumerate_opencode_store(data_dir: &Path) -> Result<Vec<StoreCandidate>, StoreError> {
    let mut candidates = enumerate_opencode_auth(&data_dir.join("auth.json"))?;
    candidates.extend(enumerate_opencode_database(&data_dir.join("opencode.db"))?);
    Ok(candidates)
}

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

#[cfg(test)]
pub(crate) fn enumerate_opencode_database(
    db_path: &Path,
) -> Result<Vec<StoreCandidate>, StoreError> {
    let Some(db) = read_store_file(db_path, DB_LIMIT)? else {
        return Ok(Vec::new());
    };
    let wal = read_store_file(&wal_sibling_path(db_path), WAL_LIMIT)?;
    parse_opencode_database(&db, wal.as_deref(), db_path)
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

#[cfg(test)]
fn parse_opencode_database(
    db: &[u8],
    wal: Option<&[u8]>,
    source: &Path,
) -> Result<Vec<StoreCandidate>, StoreError> {
    let image = SqliteImage::load(db, wal)?;
    let schema = image.walk_table(1)?;
    let Some((root, sql)) = find_table_schema(&schema, "credential") else {
        return Ok(Vec::new());
    };
    let columns = parse_column_names(&sql)?;
    let Some(value) = find_column(&columns, &VALUE_COLUMNS) else {
        return Err(StoreError::Unsupported(
            "credential table has no recognized secret column",
        ));
    };
    let map = CredentialColumns {
        value,
        provider: find_column(&columns, &PROVIDER_COLUMNS),
        method: find_column(&columns, &METHOD_COLUMNS),
        active: find_column(&columns, &ACTIVE_COLUMNS),
        label: find_column(&columns, &["label"]),
    };
    let mut candidates = Vec::new();
    for row in &image.walk_table(root)? {
        if let Some(candidate) = row_to_candidate(row, &columns, map, source) {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

#[cfg(test)]
#[derive(Debug, Clone, Copy)]
struct CredentialColumns {
    value: usize,
    provider: Option<usize>,
    method: Option<usize>,
    active: Option<usize>,
    label: Option<usize>,
}

#[cfg(test)]
fn row_to_candidate(
    row: &TableRow,
    columns: &[String],
    map: CredentialColumns,
    source: &Path,
) -> Option<StoreCandidate> {
    if map
        .active
        .and_then(|index| row.values.get(index))
        .is_some_and(|flag| !is_active(flag))
    {
        return None;
    }
    let secret = text_value(row.values.get(map.value)?)?;
    let provider_name = map
        .provider
        .and_then(|index| row.values.get(index))
        .and_then(text_value)
        .unwrap_or_else(|| format!("row-{}", row.rowid));
    let label_name = map
        .label
        .and_then(|index| row.values.get(index))
        .and_then(text_value);
    let profile = label_name.filter(|name| *name != provider_name);
    let kind = if map
        .method
        .and_then(|index| row.values.get(index))
        .and_then(text_value)
        .is_some_and(|name| {
            let folded = name.to_lowercase();
            folded.contains("oauth") || folded.contains("token")
        }) {
        CredentialKind::OAuth
    } else {
        CredentialKind::ApiKey
    };
    Some(StoreCandidate::new(
        StoreKind::Opencode,
        provider_name,
        profile,
        source.to_path_buf(),
        kind,
        columns.get(map.value)?.to_lowercase(),
        secret,
    ))
}

#[cfg(test)]
fn is_active(flag: &SqlValue) -> bool {
    match flag {
        SqlValue::Int(number) => *number != 0,
        SqlValue::Text(text) => !matches!(
            text.trim().to_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        ),
        SqlValue::Blob(bytes) => str::from_utf8(bytes).is_ok_and(|text| {
            !matches!(
                text.trim().to_lowercase().as_str(),
                "" | "0" | "false" | "no" | "off"
            )
        }),
        SqlValue::Null | SqlValue::Float(_) => true,
    }
}

#[cfg(test)]
mod tests;
