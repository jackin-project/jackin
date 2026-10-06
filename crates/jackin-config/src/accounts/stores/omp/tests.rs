// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::enumerate_omp_credentials;
use crate::accounts::stores::StoreError;
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE auth_schema_version (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    version INTEGER NOT NULL
);
INSERT INTO auth_schema_version (id, version) VALUES (1, 7);
CREATE TABLE auth_credentials (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    credential_type TEXT NOT NULL,
    data TEXT NOT NULL,
    disabled_cause TEXT DEFAULT NULL,
    identity_key TEXT DEFAULT NULL,
    created_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER)),
    updated_at INTEGER NOT NULL DEFAULT (CAST(strftime('%s','now') AS INTEGER))
);
"#;

fn store_dir(parent: &Path) -> std::path::PathBuf {
    let directory = parent.join(".omp");
    std::fs::create_dir_all(directory.join("agent")).unwrap();
    directory
}

fn write_db(source: &Path, setup: &str) {
    let connection = rusqlite::Connection::open(source.join("agent/agent.db")).unwrap();
    connection.execute_batch(setup).unwrap();
}

#[test]
fn enumerates_row_identity_in_rowid_order_without_exposing_secret_values() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    write_db(
        &source,
        &format!(
            "{SCHEMA}\n\
             INSERT INTO auth_credentials (id, provider, credential_type, data)\n\
             VALUES (7, 'anthropic', 'api_key', '{{\"key\":\"synthetic-omp-secret-one\"}}');\n\
             INSERT INTO auth_credentials (id, provider, credential_type, data)\n\
             VALUES (9, 'openai', 'oauth', '{{\"access\":\"a\",\"refresh\":\"r\",\"expires\":99}}');"
        ),
    );

    let accounts = enumerate_omp_credentials(&source).unwrap();
    let identities: Vec<_> = accounts
        .iter()
        .map(|account| (account.entry(), account.profile()))
        .collect();
    assert_eq!(
        identities,
        vec![("anthropic", "row:7"), ("openai", "row:9")]
    );
    assert!(!format!("{accounts:?}").contains("synthetic-omp-secret"));
}

#[test]
fn missing_store_yields_no_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    assert!(enumerate_omp_credentials(&source).unwrap().is_empty());
    assert!(
        enumerate_omp_credentials(&temp.path().join("absent"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn unsupported_credentials_layout_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    std::fs::write(source.join("agent/agent.db"), b"not a database").unwrap();
    assert_eq!(
        enumerate_omp_credentials(&source),
        Err(StoreError::Malformed)
    );

    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    write_db(&source, "CREATE TABLE other (id TEXT);");
    assert_eq!(
        enumerate_omp_credentials(&source),
        Err(StoreError::Malformed)
    );
}

#[test]
fn wal_committed_frames_supply_the_discovered_identity() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    write_db(
        &source,
        &format!(
            "PRAGMA journal_mode = WAL;\n\
             PRAGMA wal_autocheckpoint = 0;\n\
             {SCHEMA}\n\
             INSERT INTO auth_credentials (id, provider, credential_type, data)\n\
             VALUES (11, 'openai', 'api_key', '{{\"key\":\"synthetic-fresh\"}}');"
        ),
    );

    let accounts = enumerate_omp_credentials(&source).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].entry(), "openai");
    assert_eq!(accounts[0].profile(), "row:11");
}
