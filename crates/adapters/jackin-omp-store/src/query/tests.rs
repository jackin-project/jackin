// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use rusqlite::Connection;

use super::{OmpAccount, enumerate};
use crate::OmpError;

const SCHEMA: &str = r"
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
";

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(2)
}

fn connection() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch(SCHEMA).unwrap();
    connection
}

#[test]
fn enumerates_active_row_ids_without_formatting_credential_json() {
    let connection = connection();
    connection
            .execute_batch(
                "INSERT INTO auth_credentials
                    (id, provider, credential_type, data, identity_key, created_at, updated_at)
                 VALUES
                    (41, 'openai', 'api_key', '{\"key\":\"selected-synthetic-secret\"}', NULL, 10, 11),
                    (42, 'openai', 'oauth', '{\"access\":\"sibling-access\",\"refresh\":\"sibling-refresh\",\"expires\":99}', 'email:sibling@example.test', 12, 13),
                    (43, 'anthropic', 'api_key', '{\"key\":\"disabled-secret\"}', NULL, 14, 15);
                 UPDATE auth_credentials SET disabled_cause = 'synthetic-disabled' WHERE id = 43;",
            )
            .unwrap();

    let accounts = enumerate(&connection, deadline()).unwrap();
    assert_eq!(
        accounts,
        vec![
            OmpAccount {
                id: 41,
                entry: "openai".to_owned(),
                profile: "row:41".to_owned(),
            },
            OmpAccount {
                id: 42,
                entry: "openai".to_owned(),
                profile: "row:42".to_owned(),
            },
        ]
    );
    assert!(!format!("{accounts:?}").contains("selected-synthetic-secret"));
    assert!(!format!("{accounts:?}").contains("sibling-refresh"));
}

#[test]
fn malformed_or_unsupported_active_rows_fail_closed() {
    for (kind, data) in [
        ("api_key", "not-json"),
        ("api_key", "{\"key\":42}"),
        ("oauth", "{\"access\":\"a\",\"refresh\":\"r\"}"),
        ("future_credential", "{\"key\":\"x\"}"),
    ] {
        let connection = connection();
        connection
            .execute(
                "INSERT INTO auth_credentials
                        (provider, credential_type, data, created_at, updated_at)
                     VALUES ('openai', ?1, ?2, 1, 1)",
                rusqlite::params![kind, data],
            )
            .unwrap();
        assert_eq!(
            enumerate(&connection, deadline()),
            Err(OmpError::Unavailable),
            "active OMP row type {kind} with payload {data} must fail closed"
        );
    }
}

#[test]
fn rejects_unknown_auth_schema_version_and_column_layout() {
    let connection = connection();
    connection
        .execute(
            "UPDATE auth_schema_version SET version = 8 WHERE id = 1",
            [],
        )
        .unwrap();
    assert_eq!(
        enumerate(&connection, deadline()),
        Err(OmpError::Unavailable)
    );

    let connection = Connection::open_in_memory().unwrap();
    connection
            .execute_batch(
                "CREATE TABLE auth_schema_version (id INTEGER PRIMARY KEY CHECK (id = 1), version INTEGER NOT NULL);
                 INSERT INTO auth_schema_version VALUES (1, 7);
                 CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, data TEXT);",
            )
            .unwrap();
    assert_eq!(
        enumerate(&connection, deadline()),
        Err(OmpError::Unavailable)
    );
}
