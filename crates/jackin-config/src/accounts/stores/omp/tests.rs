// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{enumerate_omp_credentials, parse_omp_database, validate_single_credential_store};
use crate::accounts::stores::tests::{Cell, Value, database, leaf_page, wal_image};
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
fn whole_store_validator_preserves_one_account_and_rejects_ambiguity() {
    let dir = tempfile::tempdir().unwrap();
    let agent_dir = dir.path().join("agent");
    std::fs::create_dir_all(&agent_dir).unwrap();
    let path = agent_dir.join("agent.db");
    let one = credentials_db(&[Cell::row(
        1,
        vec![
            Value::Text("openai".into()),
            Value::Text("selected-sentinel".into()),
            Value::Text("work".into()),
        ],
    )]);
    std::fs::write(&path, one).unwrap();
    validate_single_credential_store(dir.path()).unwrap();

    let two = credentials_db(&[
        Cell::row(
            1,
            vec![
                Value::Text("openai".into()),
                Value::Text("selected-sentinel".into()),
                Value::Text("work".into()),
            ],
        ),
        Cell::row(
            2,
            vec![
                Value::Text("anthropic".into()),
                Value::Text("other-sentinel".into()),
                Value::Text("personal".into()),
            ],
        ),
    ]);
    std::fs::write(&path, two).unwrap();
    assert_eq!(
        validate_single_credential_store(dir.path()).unwrap_err(),
        StoreError::Unsupported("omp credential store contains multiple entries")
    );
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
