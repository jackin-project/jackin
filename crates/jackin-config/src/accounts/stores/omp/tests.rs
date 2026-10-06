// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::enumerate_omp_credentials;
use crate::accounts::stores::StoreError;
use jackin_omp_store::OmpSnapshot;
use rusqlite::Connection;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

const SCHEMA: &str = "CREATE TABLE auth_schema_version(id INTEGER PRIMARY KEY CHECK(id=1), version INTEGER NOT NULL); INSERT INTO auth_schema_version VALUES(1, 7); CREATE TABLE auth_credentials(id INTEGER PRIMARY KEY AUTOINCREMENT, provider TEXT NOT NULL, credential_type TEXT NOT NULL, data TEXT NOT NULL, disabled_cause TEXT, identity_key TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);";

fn store_dir(parent: &Path) -> std::path::PathBuf {
    let directory = parent.join(".omp");
    std::fs::create_dir_all(directory.join("agent")).unwrap();
    #[cfg(unix)]
    for path in [&directory, &directory.join("agent")] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    directory
}

fn write_credentials(source: &Path, credentials: &[(&str, &str)]) {
    let connection = Connection::open(source.join("agent/agent.db")).unwrap();
    connection.execute_batch(SCHEMA).unwrap();
    for (provider, key) in credentials {
        connection
            .execute(
                "INSERT INTO auth_credentials(provider, credential_type, data, created_at, updated_at) VALUES (?1, 'api_key', ?2, 10, 11)",
                rusqlite::params![provider, format!(r#"{{"key":"{key}"}}"#)],
            )
            .unwrap();
    }
}

#[test]
fn enumerates_row_identity_in_rowid_order_without_exposing_secret_values() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    write_credentials(&source, &[("anthropic", "a"), ("openai", "b")]);

    eprintln!(
        "debug source {} exists={:?} dir_mode={:?}",
        source.display(),
        source.exists(),
        std::fs::metadata(&source).map(|m| m.permissions().mode())
    );
    let accounts = match OmpSnapshot::capture_from_directory(&source) {
        Ok(Some(snapshot)) => snapshot.accounts().to_vec(),
        Ok(None) => panic!("snapshot unexpectedly absent"),
        Err(error) => panic!("snapshot diagnostic: {error:?}"),
    };
    let identities: Vec<_> = accounts
        .iter()
        .map(|account| (account.entry(), account.profile()))
        .collect();
    assert_eq!(
        identities,
        vec![("anthropic", "row:1"), ("openai", "row:2")]
    );
    assert!(!format!("{accounts:?}").contains("\"a\""));
    assert!(!format!("{accounts:?}").contains("\"b\""));
}

#[test]
fn missing_store_or_credentials_table_yields_no_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    assert!(enumerate_omp_credentials(&source).unwrap().is_empty());

    Connection::open(source.join("agent/agent.db"))
        .unwrap()
        .execute_batch("CREATE TABLE other (id TEXT);")
        .unwrap();
    assert_eq!(
        enumerate_omp_credentials(&source),
        Err(StoreError::Malformed)
    );
}

#[test]
fn unsupported_credentials_layout_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    Connection::open(source.join("agent/agent.db"))
        .unwrap()
        .execute_batch("CREATE TABLE credentials (id INTEGER PRIMARY KEY);")
        .unwrap();

    assert_eq!(
        enumerate_omp_credentials(&source),
        Err(StoreError::Malformed)
    );
}
