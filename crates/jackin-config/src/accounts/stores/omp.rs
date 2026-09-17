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
mod tests {
    use super::{enumerate_omp_credentials, parse_omp_database};
    use crate::accounts::stores::sqlite::fixture::{Cell, Value, database, leaf_page, wal_image};
    use crate::accounts::stores::{CredentialKind, StoreCandidate, StoreError, StoreKind};
    use std::path::Path;

    const SCHEMA: &str = "CREATE TABLE credentials (provider TEXT, value TEXT, profile TEXT)";

    fn credentials_db(cells: &[Cell]) -> Vec<u8> {
        database(SCHEMA, 2, cells, false)
    }

    #[test]
    fn selects_rows_in_rowid_order() {
        let db = credentials_db(&[
            Cell::row(
                2,
                vec![
                    Value::Text("openai".into()),
                    Value::Text("fixture-omp-002".into()),
                    Value::Null,
                ],
            ),
            Cell::row(
                1,
                vec![
                    Value::Text("anthropic".into()),
                    Value::Text("fixture-omp-001".into()),
                    Value::Text("work".into()),
                ],
            ),
            Cell::row(
                3,
                vec![
                    Value::Text("blank".into()),
                    Value::Text("   ".into()),
                    Value::Null,
                ],
            ),
        ]);
        // Input order is scrambled; the fixture lays cells sorted by rowid,
        // matching the SQLite leaf invariant the reader relies on.
        let source = Path::new("/omp/agent/agent.db");
        let candidates = parse_omp_database(&db, None, source).unwrap();
        let expected = vec![
            StoreCandidate::new(
                StoreKind::Omp,
                "anthropic".to_owned(),
                Some("work".to_owned()),
                source.to_path_buf(),
                CredentialKind::ApiKey,
                "value".to_owned(),
                "fixture-omp-001".to_owned(),
            ),
            StoreCandidate::new(
                StoreKind::Omp,
                "openai".to_owned(),
                None,
                source.to_path_buf(),
                CredentialKind::ApiKey,
                "value".to_owned(),
                "fixture-omp-002".to_owned(),
            ),
        ];
        assert_eq!(candidates, expected);
    }

    #[test]
    fn token_columns_read_as_oauth_with_rowid_fallback() {
        let db = database(
            "CREATE TABLE credentials (id INTEGER PRIMARY KEY, access_token TEXT)",
            2,
            &[Cell::row(
                7,
                vec![Value::Int(7), Value::Text("fixture-omp-003".into())],
            )],
            false,
        );
        let candidates = parse_omp_database(&db, None, Path::new("agent.db")).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].provider, "row-7");
        assert_eq!(candidates[0].kind, CredentialKind::OAuth);
        assert_eq!(candidates[0].field, "access_token");
    }

    #[test]
    fn missing_inputs_yield_no_candidates() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            enumerate_omp_credentials(&dir.path().join("agent.db"))
                .unwrap()
                .is_empty()
        );
        let no_table = database("CREATE TABLE other (id TEXT)", 2, &[], false);
        assert!(
            parse_omp_database(&no_table, None, Path::new("agent.db"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn unrecognized_secret_column_is_unsupported() {
        let db = database(
            "CREATE TABLE credentials (id INTEGER PRIMARY KEY)",
            2,
            &[],
            false,
        );
        assert_eq!(
            parse_omp_database(&db, None, Path::new("agent.db")).unwrap_err(),
            StoreError::Unsupported("credentials table has no recognized secret column")
        );
    }

    #[test]
    fn file_round_trip_labels_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.db");
        let db = credentials_db(&[Cell::row(
            1,
            vec![
                Value::Text("anthropic".into()),
                Value::Text("fixture-omp-004".into()),
                Value::Null,
            ],
        )]);
        std::fs::write(&path, &db).unwrap();
        let candidates = enumerate_omp_credentials(&path).unwrap();
        let expected = vec![StoreCandidate::new(
            StoreKind::Omp,
            "anthropic".to_owned(),
            None,
            path.clone(),
            CredentialKind::ApiKey,
            "value".to_owned(),
            "fixture-omp-004".to_owned(),
        )];
        assert_eq!(candidates, expected);
    }

    #[test]
    fn wal_sibling_file_overlays_committed_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.db");
        let stale = database(
            SCHEMA,
            2,
            &[Cell::row(
                1,
                vec![
                    Value::Text("anthropic".into()),
                    Value::Text("fixture-stale".into()),
                    Value::Null,
                ],
            )],
            true,
        );
        std::fs::write(&path, &stale).unwrap();
        let fresh = leaf_page(
            &[Cell::row(
                1,
                vec![
                    Value::Text("anthropic".into()),
                    Value::Text("fixture-fresh".into()),
                    Value::Null,
                ],
            )],
            0,
        );
        let mut wal_name = path.as_os_str().to_owned();
        wal_name.push("-wal");
        std::fs::write(&wal_name, wal_image(&[(2, 1, fresh)])).unwrap();
        let candidates = enumerate_omp_credentials(&path).unwrap();
        let expected = vec![StoreCandidate::new(
            StoreKind::Omp,
            "anthropic".to_owned(),
            None,
            path.clone(),
            CredentialKind::ApiKey,
            "value".to_owned(),
            "fixture-fresh".to_owned(),
        )];
        assert_eq!(candidates, expected);
    }
}
