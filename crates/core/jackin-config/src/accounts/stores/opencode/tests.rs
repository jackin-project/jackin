// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    enumerate_opencode_auth, enumerate_opencode_database, enumerate_opencode_store,
    validate_opencode_auth_layout,
};
use crate::accounts::stores::tests::{Cell, Value, database};
use crate::accounts::stores::{CredentialKind, StoreCandidate, StoreError, StoreKind};
use std::path::Path;

const SCHEMA: &str = "CREATE TABLE credential (id INTEGER PRIMARY KEY, integration_id TEXT, label TEXT, value TEXT, connector_id TEXT, method_id TEXT, active INTEGER)";

fn credential(cells: &[Cell], wal_mode: bool) -> Vec<u8> {
    database(SCHEMA, 2, cells, wal_mode)
}

fn row(
    rowid: u64,
    integration: &str,
    label: &str,
    value: Value,
    method: &str,
    active: Value,
) -> Cell {
    Cell::row(
        rowid,
        vec![
            Value::Int(rowid as i64),
            Value::Text(integration.into()),
            Value::Text(label.into()),
            value,
            Value::Text(String::new()),
            Value::Text(method.into()),
            active,
        ],
    )
}

#[test]
fn selects_usable_entries_sorted_by_provider() {
    let raw = r#"{
        "openai": {"type": "api", "key": "fixture-openai-001"},
        "blank": {"type": "api", "key": "   "},
        "unshaped": {"token": "fixture-no-type"},
        "anthropic": {"type": "oauth", "access": "fixture-access-001", "refresh": "fixture-refresh-001"},
        "zai": {"type": "oauth", "refresh": "fixture-refresh-002"}
    }"#;
    let source = Path::new("/data/opencode/auth.json");
    let candidates = super::parse_opencode_bytes(raw.as_bytes(), source).unwrap();
    let expected = vec![
        StoreCandidate::new(
            StoreKind::Opencode,
            "anthropic".to_owned(),
            None,
            source.to_path_buf(),
            CredentialKind::OAuth,
            "access".to_owned(),
            "fixture-access-001".to_owned(),
        ),
        StoreCandidate::new(
            StoreKind::Opencode,
            "openai".to_owned(),
            None,
            source.to_path_buf(),
            CredentialKind::ApiKey,
            "key".to_owned(),
            "fixture-openai-001".to_owned(),
        ),
        StoreCandidate::new(
            StoreKind::Opencode,
            "zai".to_owned(),
            None,
            source.to_path_buf(),
            CredentialKind::OAuth,
            "refresh".to_owned(),
            "fixture-refresh-002".to_owned(),
        ),
    ];
    assert_eq!(candidates, expected);
}

#[test]
fn selects_credential_rows_skipping_inactive() {
    let db = credential(
        &[
            row(
                1,
                "anthropic",
                "work",
                Value::Text("fixture-db-001".into()),
                "api",
                Value::Int(1),
            ),
            row(
                2,
                "openai",
                "openai",
                Value::Text("fixture-db-002".into()),
                "oauth",
                Value::Int(1),
            ),
            row(
                3,
                "xai",
                "old",
                Value::Text("fixture-db-003".into()),
                "api",
                Value::Int(0),
            ),
            row(
                4,
                "blank",
                "b",
                Value::Text("  ".into()),
                "api",
                Value::Int(1),
            ),
        ],
        false,
    );
    let source = Path::new("opencode.db");
    let candidates = super::parse_opencode_database(&db, None, source).unwrap();
    let expected = vec![
        StoreCandidate::new(
            StoreKind::Opencode,
            "anthropic".to_owned(),
            Some("work".to_owned()),
            source.to_path_buf(),
            CredentialKind::ApiKey,
            "value".to_owned(),
            "fixture-db-001".to_owned(),
        ),
        StoreCandidate::new(
            StoreKind::Opencode,
            "openai".to_owned(),
            None,
            source.to_path_buf(),
            CredentialKind::OAuth,
            "value".to_owned(),
            "fixture-db-002".to_owned(),
        ),
    ];
    assert_eq!(candidates, expected);
}

#[test]
fn store_reads_both_files_with_missing_halves_allowed() {
    let dir = tempfile::tempdir().unwrap();
    assert!(enumerate_opencode_store(dir.path()).unwrap().is_empty());
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"xai": {"type": "api", "key": "fixture-xai-001"}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("opencode.db"),
        credential(
            &[row(
                1,
                "anthropic",
                "db",
                Value::Text("fixture-db-004".into()),
                "api",
                Value::Null,
            )],
            false,
        ),
    )
    .unwrap();
    let candidates = enumerate_opencode_store(dir.path()).unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].provider, "xai");
    assert_eq!(candidates[0].source.file_name().unwrap(), "auth.json");
    assert_eq!(candidates[1].provider, "anthropic");
    assert_eq!(candidates[1].profile.as_deref(), Some("db"));
    assert_eq!(candidates[1].source.file_name().unwrap(), "opencode.db");
}

#[test]
fn auth_layout_accepts_one_entry_with_sibling_database() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"opencode-go":{"type":"api","key":"fixture-auth"}}"#,
    )
    .unwrap();
    std::fs::write(dir.path().join("opencode.db"), b"database fixture").unwrap();

    validate_opencode_auth_layout(dir.path()).unwrap();
}

#[test]
fn auth_layout_rejects_multiple_entries_before_persistence() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"anthropic":{"type":"api","key":"fixture-a"},"opencode-go":{"type":"api","key":"fixture-go"}}"#,
    )
    .unwrap();

    assert_eq!(
        validate_opencode_auth_layout(dir.path()).unwrap_err(),
        StoreError::Unsupported("OpenCode auth.json must contain exactly one provider credential")
    );
}

#[test]
fn auth_layout_rejects_single_foreign_entry_without_usage_materialization() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("auth.json"),
        r#"{"zai":{"type":"api","key":"fixture-zai"}}"#,
    )
    .unwrap();

    assert_eq!(
        validate_opencode_auth_layout(dir.path()).unwrap_err(),
        StoreError::Unsupported(
            "OpenCode source-bound profiles currently support only the opencode-go auth entry"
        )
    );
}

#[test]
fn rejects_non_object_and_invalid_json() {
    for raw in [r"[]", r"null", r#""str""#, r#"{"a": "#] {
        let result = super::parse_opencode_bytes(raw.as_bytes(), Path::new("auth.json"));
        assert_eq!(result.unwrap_err(), StoreError::Malformed, "raw: {raw}");
    }
}

#[test]
fn missing_inputs_yield_no_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("auth.json");
    assert!(enumerate_opencode_auth(&missing).unwrap().is_empty());
    assert!(
        enumerate_opencode_database(&dir.path().join("opencode.db"))
            .unwrap()
            .is_empty()
    );
    let no_table = database("CREATE TABLE other (id TEXT)", 2, &[], false);
    assert!(
        super::parse_opencode_database(&no_table, None, Path::new("opencode.db"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unrecognized_secret_column_is_unsupported() {
    let db = database(
        "CREATE TABLE credential (id INTEGER PRIMARY KEY)",
        2,
        &[],
        false,
    );
    assert_eq!(
        super::parse_opencode_database(&db, None, Path::new("opencode.db")).unwrap_err(),
        StoreError::Unsupported("credential table has no recognized secret column")
    );
}
