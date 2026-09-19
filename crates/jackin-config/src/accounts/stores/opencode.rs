// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Read-only enumerator for XDG data `opencode` credential stores.
//!
//! Authentication material lives in two sibling files, and either may be
//! absent:
//!
//! - `auth.json`: a provider-keyed JSON object; each entry is either
//!   `{ "type": "api", "key": ... }` or `{ "type": "oauth", "access": ...,
//!   "refresh": ... }`;
//! - `opencode.db`: a `SQLite` database whose `credential` table carries one
//!   row per credential with columns such as `id`, `integration_id`,
//!   `label`, `value`, `connector_id`, `method_id`, and `active`.
//!
//! Enumeration selects only entries holding a non-blank secret and returns
//! one [`StoreCandidate`](super::StoreCandidate) per entry: `auth.json`
//! providers sorted by name first, then `credential` rows in rowid order.
//! The whole store is never copied: blank or unshaped entries are skipped,
//! and only the selected secrets are cloned into candidates.
//!
//! The database is parsed through the shared [`sqlite`](super::sqlite)
//! reader (no `rusqlite`, no Turso, no writes, no checkpoints). Rows whose
//! `active` column is explicitly falsy (`0`, `"false"`, `"no"`, `"off"`,
//! blank) are skipped; rows without the column are kept.

use std::path::Path;

use super::sqlite::{
    SqlValue, SqliteImage, TableRow, find_column, find_table_schema, parse_column_names,
    text_value, wal_sibling_path,
};
use super::{
    CredentialKind, StoreCandidate, StoreError, StoreKind, read_store_file, select_entry_secret,
};

/// Maximum `auth.json` size accepted for enumeration.
const AUTH_JSON_LIMIT: u64 = 1024 * 1024;
/// Maximum database file size accepted for enumeration.
const DB_LIMIT: u64 = 8 * 1024 * 1024;
/// Maximum `-wal` sibling size accepted for enumeration.
const WAL_LIMIT: u64 = 8 * 1024 * 1024;

/// `credential` columns holding the secret, in preference order.
const VALUE_COLUMNS: [&str; 1] = ["value"];
/// `credential` columns naming the provider, in preference order.
const PROVIDER_COLUMNS: [&str; 3] = ["integration_id", "connector_id", "label"];
/// `credential` columns naming the credential method, in preference order.
const METHOD_COLUMNS: [&str; 2] = ["method_id", "method"];
/// `credential` columns flagging whether the row is active.
const ACTIVE_COLUMNS: [&str; 1] = ["active"];

/// Enumerate usable credentials from an `opencode` data directory.
///
/// Reads the sibling `auth.json` and `opencode.db` files when present;
/// absent files yield no candidates for their half. Pure parsing: no
/// scanning, no writes.
///
/// # Errors
///
/// Returns [`StoreError`] when a present source is unreadable, oversized, or
/// malformed. One corrupt source fails the whole enumeration rather than
/// silently hiding half the store.
pub(crate) fn enumerate_opencode_store(data_dir: &Path) -> Result<Vec<StoreCandidate>, StoreError> {
    let mut candidates = enumerate_opencode_auth(&data_dir.join("auth.json"))?;
    candidates.extend(enumerate_opencode_database(&data_dir.join("opencode.db"))?);
    Ok(candidates)
}

/// Enumerate usable provider credentials from an `opencode/auth.json` file.
///
/// A missing file yields no candidates; an unreadable, oversized, or
/// non-object file fails. Pure parsing: no scanning, no writes.
///
/// # Errors
///
/// Returns [`StoreError`] for unreadable, oversized, or malformed sources.
pub(crate) fn enumerate_opencode_auth(auth_path: &Path) -> Result<Vec<StoreCandidate>, StoreError> {
    let Some(bytes) = read_store_file(auth_path, AUTH_JSON_LIMIT)? else {
        return Ok(Vec::new());
    };
    parse_opencode_bytes(&bytes, auth_path)
}

/// Enumerate usable credentials from the `credential` table of an
/// `opencode.db` file.
///
/// A missing file yields no candidates, as does a database without a
/// `credential` table. Pure parsing from the given path plus its `-wal`
/// sibling: no scanning, no writes, no checkpoints.
///
/// # Errors
///
/// Returns [`StoreError`] for unreadable, oversized, malformed, or
/// unsupported-layout sources.
pub(crate) fn enumerate_opencode_database(
    db_path: &Path,
) -> Result<Vec<StoreCandidate>, StoreError> {
    let Some(db) = read_store_file(db_path, DB_LIMIT)? else {
        return Ok(Vec::new());
    };
    let wal = read_store_file(&wal_sibling_path(db_path), WAL_LIMIT)?;
    parse_opencode_database(&db, wal.as_deref(), db_path)
}

/// Parse provider-keyed `auth.json` bytes into per-provider candidates.
///
/// `source` labels the resulting candidates; it is not read.
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

/// Resolved `credential` column positions for one table walk.
#[derive(Debug, Clone, Copy)]
struct CredentialColumns {
    value: usize,
    provider: Option<usize>,
    method: Option<usize>,
    active: Option<usize>,
    label: Option<usize>,
}

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

/// Whether an `active` flag reads as enabled. Only explicitly falsy values
/// disable the row; NULL and unrecognized shapes keep it.
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
