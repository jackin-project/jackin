// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Read-only enumerator for `.omp/agent/agent.db` credentials.
//!
//! Parses the `SQLite` database through the shared [`sqlite`](super::sqlite)
//! reader (no `rusqlite`, no Turso, no writes, no checkpoints) and selects
//! rows from exactly one table:
//!
//! ```sql
//! CREATE TABLE credentials (...);
//! ```
//!
//! Column names are matched case-insensitively against small preference
//! lists; see [`VALUE_COLUMNS`], [`PROVIDER_COLUMNS`], and
//! [`PROFILE_COLUMNS`]. One candidate is emitted per row holding a non-blank
//! value, in rowid order. Rows without a provider name fall back to
//! `row-<rowid>` so distinct rows stay distinguishable.

use std::path::Path;

use super::sqlite::{
    SqliteImage, TableRow, find_column, find_table_schema, parse_column_names, text_value,
    wal_sibling_path,
};
use super::{CredentialKind, StoreCandidate, StoreError, StoreKind, read_store_file};

/// Maximum database file size accepted for enumeration.
const DB_LIMIT: u64 = 8 * 1024 * 1024;
/// Maximum `-wal` sibling size accepted for enumeration.
const WAL_LIMIT: u64 = 8 * 1024 * 1024;

/// `credentials` columns holding the secret, in preference order.
const VALUE_COLUMNS: [&str; 8] = [
    "value",
    "secret",
    "token",
    "key",
    "access_token",
    "api_key",
    "credential",
    "password",
];
/// `credentials` columns naming the provider, in preference order.
const PROVIDER_COLUMNS: [&str; 6] = ["provider", "service", "scope", "account", "name", "key_id"];
/// `credentials` columns naming the profile, in preference order.
const PROFILE_COLUMNS: [&str; 2] = ["profile", "label"];
/// Value columns whose contents read as subscription tokens.
const TOKEN_COLUMNS: [&str; 2] = ["token", "access_token"];

/// Enumerate usable credentials from an `.omp/agent/agent.db` file.
///
/// A missing file yields no candidates, as does a database without a
/// `credentials` table. Pure parsing from the given path plus its `-wal`
/// sibling: no scanning, no writes, no checkpoints.
///
/// # Errors
///
/// Returns [`StoreError`] for unreadable, oversized, malformed, or
/// unsupported-layout sources.
pub(crate) fn enumerate_omp_credentials(db_path: &Path) -> Result<Vec<StoreCandidate>, StoreError> {
    let Some(db) = read_store_file(db_path, DB_LIMIT)? else {
        return Ok(Vec::new());
    };
    let wal = read_store_file(&wal_sibling_path(db_path), WAL_LIMIT)?;
    parse_omp_database(&db, wal.as_deref(), db_path)
}

/// Prove that an Omp store has exactly one usable credential entry before a
/// whole-database sync is allowed. The runtime has no safe `SQLite` writer for
/// filtering arbitrary schemas, so ambiguous stores fail closed.
pub(crate) fn validate_single_credential_store(dir: &Path) -> Result<(), StoreError> {
    let candidates = enumerate_omp_credentials(&dir.join("agent/agent.db"))?;
    if candidates.len() == 1 {
        Ok(())
    } else if candidates.is_empty() {
        Err(StoreError::Unsupported(
            "omp credential store has no usable entry",
        ))
    } else {
        Err(StoreError::Unsupported(
            "omp credential store contains multiple entries",
        ))
    }
}

fn parse_omp_database(
    db: &[u8],
    wal: Option<&[u8]>,
    source: &Path,
) -> Result<Vec<StoreCandidate>, StoreError> {
    let image = SqliteImage::load(db, wal)?;
    let schema = image.walk_table(1)?;
    let Some((root, sql)) = find_table_schema(&schema, "credentials") else {
        return Ok(Vec::new());
    };
    parse_credential_rows(&image, root, &sql, source)
}

/// Select candidates from the decoded `credentials` table.
fn parse_credential_rows(
    image: &SqliteImage,
    root: u32,
    sql: &str,
    source: &Path,
) -> Result<Vec<StoreCandidate>, StoreError> {
    let columns = parse_column_names(sql)?;
    let Some(value) = find_column(&columns, &VALUE_COLUMNS) else {
        return Err(StoreError::Unsupported(
            "credentials table has no recognized secret column",
        ));
    };
    let provider = find_column(&columns, &PROVIDER_COLUMNS);
    let profile = find_column(&columns, &PROFILE_COLUMNS);
    let mut candidates = Vec::new();
    for row in &image.walk_table(root)? {
        if let Some(candidate) = row_to_candidate(row, &columns, value, provider, profile, source) {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

fn row_to_candidate(
    row: &TableRow,
    columns: &[String],
    value: usize,
    provider: Option<usize>,
    profile: Option<usize>,
    source: &Path,
) -> Option<StoreCandidate> {
    let secret = text_value(row.values.get(value)?)?;
    let provider_name = provider
        .and_then(|index| row.values.get(index))
        .and_then(text_value)
        .unwrap_or_else(|| format!("row-{}", row.rowid));
    let profile_name = profile
        .and_then(|index| row.values.get(index))
        .and_then(text_value);
    let column = columns.get(value)?.to_lowercase();
    let kind = if TOKEN_COLUMNS.contains(&column.as_str()) {
        CredentialKind::OAuth
    } else {
        CredentialKind::ApiKey
    };
    Some(StoreCandidate::new(
        StoreKind::Omp,
        provider_name,
        profile_name,
        source.to_path_buf(),
        kind,
        column,
        secret,
    ))
}

#[cfg(test)]
mod tests;
