// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::enumerate_omp_credentials;
use crate::accounts::stores::tests::{Cell, Value, database, leaf_page, wal_image};
use crate::accounts::stores::StoreError;
use std::path::Path;

const SCHEMA: &str = "CREATE TABLE credentials (provider TEXT, value TEXT, profile TEXT)";

fn store_dir(parent: &Path) -> std::path::PathBuf {
    let directory = parent.join(".omp");
    std::fs::create_dir_all(directory.join("agent")).unwrap();
    directory
}

fn write_db(source: &Path, bytes: &[u8]) {
    std::fs::write(source.join("agent/agent.db"), bytes).unwrap();
}

#[test]
fn enumerates_row_identity_in_rowid_order_without_exposing_secret_values() {
    let database = database(
        SCHEMA,
        2,
        &[
            Cell::row(
                1,
                vec![
                    Value::Text("anthropic".into()),
                    Value::Text("synthetic-omp-secret-one".into()),
                    Value::Text("work".into()),
                ],
            ),
            Cell::row(
                2,
                vec![
                    Value::Text("openai".into()),
                    Value::Text("synthetic-omp-secret-two".into()),
                    Value::Null,
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
        ],
        false,
    );
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    write_db(&source, &database);

    let accounts = enumerate_omp_credentials(&source).unwrap();
    let identities: Vec<_> = accounts
        .iter()
        .map(|account| (account.entry(), account.profile()))
        .collect();
    assert_eq!(
        identities,
        vec![("anthropic", Some("work")), ("openai", None)]
    );
    assert!(!format!("{accounts:?}").contains("synthetic-omp-secret"));
}

#[test]
fn missing_store_or_credentials_table_yields_no_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    assert!(enumerate_omp_credentials(&source).unwrap().is_empty());

    let no_credentials_table = database("CREATE TABLE other (id TEXT)", 2, &[], false);
    write_db(&source, &no_credentials_table);
    assert!(enumerate_omp_credentials(&source).unwrap().is_empty());
}

#[test]
fn unsupported_credentials_layout_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    let database = database(
        "CREATE TABLE credentials (id INTEGER PRIMARY KEY)",
        2,
        &[],
        false,
    );
    write_db(&source, &database);

    assert_eq!(
        enumerate_omp_credentials(&source),
        Err(StoreError::Malformed)
    );
}

#[test]
fn wal_committed_frames_supply_the_discovered_identity() {
    let temp = tempfile::tempdir().unwrap();
    let source = store_dir(temp.path());
    let stale = database(
        SCHEMA,
        2,
        &[Cell::row(
            1,
            vec![
                Value::Text("openai".into()),
                Value::Text("synthetic-stale".into()),
                Value::Text("work".into()),
            ],
        )],
        true,
    );
    write_db(&source, &stale);
    let fresh = leaf_page(
        &[Cell::row(
            1,
            vec![
                Value::Text("openai".into()),
                Value::Text("synthetic-fresh".into()),
                Value::Text("work".into()),
            ],
        )],
        0,
    );
    std::fs::write(
        source.join("agent/agent.db-wal"),
        wal_image(&[(2, 1, fresh)]),
    )
    .unwrap();

    let accounts = enumerate_omp_credentials(&source).unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].entry(), "openai");
    assert_eq!(accounts[0].profile(), Some("work"));
}
