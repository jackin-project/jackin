// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::extract_omp_api_key;

use super::support::TestResult;

#[test]
fn omp_api_key_comes_from_the_selected_providers_sqlite_row() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("agent.db");
    let connection = rusqlite::Connection::open(&path)?;
    connection.execute_batch(
        "CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY AUTOINCREMENT, provider TEXT NOT NULL, credential_type TEXT NOT NULL, data TEXT NOT NULL, disabled_cause TEXT DEFAULT NULL, identity_key TEXT DEFAULT NULL, created_at INTEGER NOT NULL DEFAULT 0, updated_at INTEGER NOT NULL DEFAULT 0);
         CREATE TABLE decoy (blob TEXT NOT NULL);
         INSERT INTO decoy (blob) VALUES ('wanted {\"key\": \"fake-decoy-key\"}');
         INSERT INTO auth_credentials (provider, credential_type, data) VALUES ('other', 'api_key', '{\"key\": \"fake-other-key\"}');
         INSERT INTO auth_credentials (provider, credential_type, data) VALUES ('wanted', 'oauth', '{\"access\": \"fake-oauth\"}');
         INSERT INTO auth_credentials (provider, credential_type, data) VALUES ('wanted', 'api_key', '{\"key\": \"fake-wanted-key\", \"source\": \"test\"}');",
    )?;
    drop(connection);
    let key = extract_omp_api_key(&path, "wanted").ok_or("missing wanted key")?;
    assert_eq!(key.expose(), "fake-wanted-key");
    assert!(extract_omp_api_key(&path, "absent").is_none());
    Ok(())
}
