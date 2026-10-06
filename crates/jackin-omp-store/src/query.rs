// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Secret-free identities and exact credential rows for OMP's auth store.

use std::time::Instant;

use rusqlite::{Connection, OptionalExtension, types::ValueRef};
use zeroize::Zeroizing;

use crate::OmpError;

pub(crate) const AUTH_SCHEMA_VERSION: i64 = 7;
const MAX_COLUMNS: usize = 8;
const MAX_ROWS: usize = 256;
const MAX_CREDENTIAL_JSON_BYTES: usize = 64 * 1024;

const AUTH_CREDENTIALS_COLUMNS: [(&str, &str, i64, i64); MAX_COLUMNS] = [
    ("id", "INTEGER", 0, 1),
    ("provider", "TEXT", 1, 0),
    ("credential_type", "TEXT", 1, 0),
    ("data", "TEXT", 1, 0),
    ("disabled_cause", "TEXT", 0, 0),
    ("identity_key", "TEXT", 0, 0),
    ("created_at", "INTEGER", 1, 0),
    ("updated_at", "INTEGER", 1, 0),
];

/// One active OMP credential row, represented only by its provider and row ID.
#[derive(Clone, PartialEq, Eq)]
pub struct OmpAccount {
    pub(crate) id: i64,
    pub(crate) entry: String,
    pub(crate) profile: String,
}

impl OmpAccount {
    /// Provider key stored in `auth_credentials.provider`.
    pub fn entry(&self) -> &str {
        &self.entry
    }

    /// Stable source-row selector, not an OMP display profile.
    pub fn profile(&self) -> &str {
        &self.profile
    }
}

impl std::fmt::Debug for OmpAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OmpAccount([REDACTED])")
    }
}

/// Exact provider/account identity used to select one row for a role.
#[derive(Clone, PartialEq, Eq)]
pub struct OmpSelector {
    /// Exact provider key in the source database.
    pub entry: String,
    /// `row:<id>` from the same source database.
    pub profile: Option<String>,
}

impl std::fmt::Debug for OmpSelector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OmpSelector([REDACTED])")
    }
}

/// Selected OMP credential data. It never implements `Debug` and its JSON
/// payload is zeroized when the selection is dropped.
pub(crate) struct OmpCredential {
    pub(crate) id: i64,
    pub(crate) provider: String,
    pub(crate) credential_type: String,
    pub(crate) data: Zeroizing<String>,
    pub(crate) identity_key: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
}

/// Enumerate active rows from the exact OMP v7 schema without returning or
/// retaining credential JSON. The SQLite connection is a private captured
/// copy and has already been given resource limits and a progress deadline.
pub(crate) fn enumerate(
    connection: &Connection,
    deadline: Instant,
) -> Result<Vec<OmpAccount>, OmpError> {
    validate_supported_schema(connection, deadline)?;
    let mut statement = connection
        .prepare(ACTIVE_ROWS_SQL)
        .map_err(|_| OmpError::Unavailable)?;
    let mut rows = statement
        .query([i64::try_from(MAX_ROWS + 1).unwrap_or(i64::MAX)])
        .map_err(|_| OmpError::Unavailable)?;
    let mut accounts = Vec::new();
    while let Some(row) = rows.next().map_err(|_| OmpError::Unavailable)? {
        check_deadline(deadline)?;
        if accounts.len() == MAX_ROWS {
            return Err(OmpError::LimitExceeded);
        }
        let credential = read_credential(row, 7)?;
        let id = credential.id;
        let provider = credential.provider.clone();
        accounts.push(OmpAccount {
            id,
            entry: provider,
            profile: row_profile(id),
        });
        // The query only needs row identity. Drop the temporary, zeroized
        // payload rather than retaining credentials during discovery.
        drop(credential);
    }
    check_deadline(deadline)?;
    Ok(accounts)
}

/// Read one selected active row from the same immutable captured image.
pub(crate) fn selected_credential(
    connection: &Connection,
    account: &OmpAccount,
    deadline: Instant,
) -> Result<OmpCredential, OmpError> {
    validate_supported_schema(connection, deadline)?;
    let mut statement = connection
        .prepare(SELECTED_ROW_SQL)
        .map_err(|_| OmpError::Unavailable)?;
    let mut rows = statement
        .query([account.id])
        .map_err(|_| OmpError::Unavailable)?;
    let Some(row) = rows.next().map_err(|_| OmpError::Unavailable)? else {
        return Err(OmpError::SelectionUnavailable);
    };
    let credential = read_credential(row, 7)?;
    if credential.provider != account.entry || row_profile(credential.id) != account.profile {
        return Err(OmpError::SelectionUnavailable);
    }
    if rows.next().map_err(|_| OmpError::Unavailable)?.is_some() {
        return Err(OmpError::SelectionUnavailable);
    }
    check_deadline(deadline)?;
    Ok(credential)
}

fn validate_supported_schema(connection: &Connection, deadline: Instant) -> Result<(), OmpError> {
    check_deadline(deadline)?;
    let version = connection
        .query_row(
            "SELECT version FROM main.auth_schema_version WHERE id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map_err(|_| OmpError::Unavailable)?;
    if version != Some(AUTH_SCHEMA_VERSION) {
        return Err(OmpError::Unavailable);
    }

    let object_type = connection
        .query_row(
            "SELECT type FROM main.sqlite_master WHERE name = 'auth_credentials'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| OmpError::Unavailable)?;
    if object_type.as_deref() != Some("table") {
        return Err(OmpError::Unavailable);
    }

    let mut statement = connection
        .prepare("PRAGMA main.table_xinfo('auth_credentials')")
        .map_err(|_| OmpError::Unavailable)?;
    let mut rows = statement.query([]).map_err(|_| OmpError::Unavailable)?;
    let mut columns = 0usize;
    while let Some(row) = rows.next().map_err(|_| OmpError::Unavailable)? {
        check_deadline(deadline)?;
        if columns == MAX_COLUMNS {
            return Err(OmpError::Unavailable);
        }
        let name: String = row.get(1).map_err(|_| OmpError::Unavailable)?;
        let declared_type: String = row.get(2).map_err(|_| OmpError::Unavailable)?;
        let not_null: i64 = row.get(3).map_err(|_| OmpError::Unavailable)?;
        let primary_key: i64 = row.get(5).map_err(|_| OmpError::Unavailable)?;
        let hidden: i64 = row.get(6).map_err(|_| OmpError::Unavailable)?;
        let expected = AUTH_CREDENTIALS_COLUMNS[columns];
        if name != expected.0
            || !declared_type.eq_ignore_ascii_case(expected.1)
            || not_null != expected.2
            || primary_key != expected.3
            || hidden != 0
        {
            return Err(OmpError::Unavailable);
        }
        columns += 1;
    }
    if columns != MAX_COLUMNS {
        return Err(OmpError::Unavailable);
    }
    check_deadline(deadline)
}

fn read_credential(
    row: &rusqlite::Row<'_>,
    validity_column: usize,
) -> Result<OmpCredential, OmpError> {
    let id: i64 = row.get(0).map_err(|_| OmpError::Unavailable)?;
    if id <= 0 {
        return Err(OmpError::Unavailable);
    }
    let provider = owned_text(row.get_ref(1).map_err(|_| OmpError::Unavailable)?)?
        .filter(|provider| !provider.trim().is_empty())
        .ok_or(OmpError::Unavailable)?;
    let credential_type = owned_text(row.get_ref(2).map_err(|_| OmpError::Unavailable)?)?
        .ok_or(OmpError::Unavailable)?;
    if credential_type != "api_key" && credential_type != "oauth" {
        return Err(OmpError::Unavailable);
    }
    let data = match row.get_ref(3).map_err(|_| OmpError::Unavailable)? {
        ValueRef::Text(bytes) if bytes.len() <= MAX_CREDENTIAL_JSON_BYTES => Zeroizing::new(
            std::str::from_utf8(bytes)
                .map_err(|_| OmpError::Unavailable)?
                .to_owned(),
        ),
        ValueRef::Text(_) => return Err(OmpError::LimitExceeded),
        ValueRef::Null | ValueRef::Blob(_) | ValueRef::Integer(_) | ValueRef::Real(_) => {
            return Err(OmpError::Unavailable);
        }
    };
    let identity_key = match row.get_ref(4).map_err(|_| OmpError::Unavailable)? {
        ValueRef::Null => None,
        value => Some(owned_text(value)?.ok_or(OmpError::Unavailable)?),
    };
    let created_at = integer_value(row.get_ref(5).map_err(|_| OmpError::Unavailable)?)?;
    let updated_at = integer_value(row.get_ref(6).map_err(|_| OmpError::Unavailable)?)?;
    let usable: i64 = row
        .get(validity_column)
        .map_err(|_| OmpError::Unavailable)?;
    if usable != 1 {
        return Err(OmpError::Unavailable);
    }
    Ok(OmpCredential {
        id,
        provider,
        credential_type,
        data,
        identity_key,
        created_at,
        updated_at,
    })
}

fn owned_text(value: ValueRef<'_>) -> Result<Option<String>, OmpError> {
    match value {
        ValueRef::Text(bytes) => std::str::from_utf8(bytes)
            .map(|text| Some(text.to_owned()))
            .map_err(|_| OmpError::Unavailable),
        ValueRef::Null => Ok(None),
        ValueRef::Integer(_) | ValueRef::Real(_) | ValueRef::Blob(_) => Err(OmpError::Unavailable),
    }
}

fn integer_value(value: ValueRef<'_>) -> Result<i64, OmpError> {
    match value {
        ValueRef::Integer(value) => Ok(value),
        ValueRef::Null | ValueRef::Real(_) | ValueRef::Text(_) | ValueRef::Blob(_) => {
            Err(OmpError::Unavailable)
        }
    }
}

fn row_profile(id: i64) -> String {
    format!("row:{id}")
}

fn check_deadline(deadline: Instant) -> Result<(), OmpError> {
    if Instant::now() >= deadline {
        Err(OmpError::Deadline)
    } else {
        Ok(())
    }
}

const ACTIVE_ROWS_SQL: &str = r#"
SELECT id, provider, credential_type, data, identity_key, created_at, updated_at,
       CASE
         WHEN typeof(data) != 'text' OR json_valid(data) != 1 THEN 0
         WHEN json_type(data) != 'object' THEN 0
         WHEN credential_type = 'api_key' THEN
           CASE WHEN json_type(data, '$.key') = 'text'
                     AND length(json_extract(data, '$.key')) > 0 THEN 1 ELSE 0 END
         WHEN credential_type = 'oauth' THEN
           CASE WHEN json_type(data, '$.access') = 'text'
                     AND length(json_extract(data, '$.access')) > 0
                     AND json_type(data, '$.refresh') = 'text'
                     AND length(json_extract(data, '$.refresh')) > 0
                     AND json_type(data, '$.expires') IN ('integer', 'real')
                THEN 1 ELSE 0 END
         ELSE 0
       END AS usable
FROM main.auth_credentials
WHERE disabled_cause IS NULL
ORDER BY id
LIMIT ?1
"#;

const SELECTED_ROW_SQL: &str = r#"
SELECT id, provider, credential_type, data, identity_key, created_at, updated_at,
       CASE
         WHEN typeof(data) != 'text' OR json_valid(data) != 1 THEN 0
         WHEN json_type(data) != 'object' THEN 0
         WHEN credential_type = 'api_key' THEN
           CASE WHEN json_type(data, '$.key') = 'text'
                     AND length(json_extract(data, '$.key')) > 0 THEN 1 ELSE 0 END
         WHEN credential_type = 'oauth' THEN
           CASE WHEN json_type(data, '$.access') = 'text'
                     AND length(json_extract(data, '$.access')) > 0
                     AND json_type(data, '$.refresh') = 'text'
                     AND length(json_extract(data, '$.refresh')) > 0
                     AND json_type(data, '$.expires') IN ('integer', 'real')
                THEN 1 ELSE 0 END
         ELSE 0
       END AS usable
FROM main.auth_credentials
WHERE id = ?1 AND disabled_cause IS NULL
"#;

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use rusqlite::Connection;

    use super::{OmpAccount, enumerate};
    use crate::OmpError;

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
}
