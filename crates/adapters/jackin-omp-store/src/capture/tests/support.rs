// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const SOURCE_SCHEMA: &str = r"
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
CREATE INDEX idx_auth_provider ON auth_credentials(provider);
CREATE INDEX idx_auth_provider_identity
    ON auth_credentials(provider, identity_key) WHERE identity_key IS NOT NULL;
";

pub(super) fn source_directory(parent: &Path) -> std::path::PathBuf {
    let source = parent.join(".omp");
    fs::create_dir_all(source.join("agent")).unwrap();
    source
}

pub(super) fn create_source(parent: &Path, sibling_rows: bool) -> (std::path::PathBuf, Connection) {
    let source = source_directory(parent);
    let database = source.join("agent/agent.db");
    let connection = Connection::open(database).unwrap();
    connection
        .execute_batch(
            "PRAGMA page_size = 512;
                 PRAGMA journal_mode = WAL;
                 PRAGMA wal_autocheckpoint = 0;
                 PRAGMA secure_delete = OFF;",
        )
        .unwrap();
    connection.execute_batch(SOURCE_SCHEMA).unwrap();
    connection
        .execute(
            "INSERT INTO auth_credentials
                    (id, provider, credential_type, data, identity_key, created_at, updated_at)
                 VALUES (41, 'openai', 'api_key', ?1, NULL, 10, 11)",
            [r#"{"key":"selected-old-canary"}"#],
        )
        .unwrap();
    if sibling_rows {
        connection
            .execute(
                "INSERT INTO auth_credentials
                        (id, provider, credential_type, data, identity_key, created_at, updated_at)
                     VALUES (42, 'openai', 'api_key', ?1, NULL, 12, 13)",
                [r#"{"key":"same-provider-sibling-canary"}"#],
            )
            .unwrap();
        connection
                .execute(
                    "INSERT INTO auth_credentials
                        (id, provider, credential_type, data, identity_key, created_at, updated_at)
                     VALUES (43, 'anthropic', 'oauth', ?1, 'email:other@example.test', 14, 15)",
                    [r#"{"access":"other-access-canary","refresh":"other-refresh-canary","expires":99}"#],
                )
                .unwrap();
    }
    Ok::<(), rusqlite::Error>(())
        .and_then(|()| {
            connection.execute_batch("CREATE TABLE unrelated_data(value TEXT);")?;
            connection.execute(
                "INSERT INTO unrelated_data(value) VALUES ('unrelated-table-canary')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    (source, connection)
}

pub(super) fn selector(id: i64) -> OmpSelector {
    OmpSelector {
        entry: "openai".to_owned(),
        profile: Some(format!("row:{id}")),
    }
}

pub(super) fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

pub(super) fn checkpoint(connection: &Connection) {
    let _: (i64, i64, i64) = connection
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap();
}

pub(super) fn full_checkpoint(connection: &Connection) -> (i64, i64, i64) {
    connection.busy_timeout(Duration::from_millis(50)).unwrap();
    connection
        .query_row("PRAGMA wal_checkpoint(FULL)", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .unwrap()
}

pub(super) fn export_selected(snapshot: &mut OmpSnapshot, id: i64) -> zeroize::Zeroizing<Vec<u8>> {
    let selected = snapshot
        .select(Some("openai"), Some(&selector(id)))
        .unwrap();
    let mut output = zeroize::Zeroizing::new(Vec::new());
    selected.write_standalone_database(&mut *output).unwrap();
    output
}

pub(super) fn assert_exported_selected_row(bytes: &[u8], export_path: &Path, expected_data: &str) {
    fs::write(export_path, bytes).unwrap();
    let connection = Connection::open(export_path).unwrap();
    let mut statement = connection
        .prepare("SELECT id, provider, credential_type, data FROM auth_credentials ORDER BY id")
        .unwrap();
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![(
            41,
            "openai".to_owned(),
            "api_key".to_owned(),
            expected_data.to_owned()
        )]
    );
    let tables = connection
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(tables, vec!["auth_credentials", "auth_schema_version"]);
}

pub(super) fn write_uncommitted_spill(connection: &Connection) {
    connection
        .execute_batch(
            "CREATE TABLE spill_pages(value TEXT); BEGIN IMMEDIATE; PRAGMA cache_size = 10;",
        )
        .unwrap();
    for index in 0..100 {
        let value = format!("uncommitted-spill-{index}-{}", "x".repeat(2_000));
        connection
            .execute("INSERT INTO spill_pages(value) VALUES (?1)", [value])
            .unwrap();
    }
}
