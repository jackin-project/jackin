// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::Write as _;
use std::path::Path;

use jackin_core::{FileMode, FileSystem, HostPath, OMP_CAPTURE_LIMITS, StdFileSystem};
use jackin_session::read_omp_host_key;
use jackin_testkit::MemFs;

use super::support::TestResult;

#[tokio::test]
async fn omp_api_key_comes_from_the_selected_providers_sqlite_row() -> TestResult {
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
    let host = HostPath::parse(&std::fs::canonicalize(&path)?)?;
    let fs = StdFileSystem::default();
    let key = read_omp_host_key(&fs, &host, "wanted").await?;
    assert_eq!(key.expose(), "fake-wanted-key");
    read_omp_host_key(&fs, &host, "absent").await.unwrap_err();
    Ok(())
}

#[tokio::test]
async fn an_omp_store_behind_a_symbolic_link_is_not_followed() -> TestResult {
    let dir = tempfile::tempdir()?;
    let real = dir.path().join("real.db");
    let connection = rusqlite::Connection::open(&real)?;
    connection.execute_batch(
        "CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, credential_type TEXT, data TEXT);
         INSERT INTO auth_credentials (provider, credential_type, data) VALUES ('wanted', 'api_key', '{\"key\": \"fake-wanted-key\"}');",
    )?;
    drop(connection);
    let link = dir.path().join("agent.db");
    std::os::unix::fs::symlink(&real, &link)?;
    let fs = StdFileSystem::default();
    read_omp_host_key(&fs, &HostPath::parse(&link)?, "wanted")
        .await
        .unwrap_err();
    let key = read_omp_host_key(
        &fs,
        &HostPath::parse(&std::fs::canonicalize(&real)?)?,
        "wanted",
    )
    .await?;
    assert_eq!(key.expose(), "fake-wanted-key");
    Ok(())
}

#[tokio::test]
async fn an_omp_store_over_its_size_cap_is_unreadable() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("agent.db");
    let mut file = std::fs::File::create(&path)?;
    file.write_all(jackin_core::SQLITE_MAGIC)?;
    file.set_len(u64::try_from(OMP_CAPTURE_LIMITS.database_bytes)? + 1)?;
    drop(file);
    read_omp_host_key(
        &StdFileSystem::default(),
        &HostPath::parse(&std::fs::canonicalize(&path)?)?,
        "wanted",
    )
    .await
    .unwrap_err();
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn an_omp_store_read_that_never_returns_is_given_up_on() -> TestResult {
    let fs = MemFs::new();
    let store = HostPath::parse(Path::new("/home/work/.omp/agent/agent.db"))?;
    fs.create_dir_all(&HostPath::parse(Path::new("/home/work/.omp/agent"))?)
        .await?;
    fs.write_new(&store, jackin_core::SQLITE_MAGIC, FileMode(0o600))
        .await?;
    let probe = fs.stall_reads_of("agent.db");
    read_omp_host_key(&fs, &store, "wanted").await.unwrap_err();
    assert_eq!(probe.entered(), 1);
    assert_eq!(probe.abandoned(), 1);
    Ok(())
}

fn view_store(dir: &Path, view: &str) -> Result<HostPath, Box<dyn std::error::Error>> {
    let path = dir.join("agent.db");
    let connection = rusqlite::Connection::open(&path)?;
    connection.execute_batch(view)?;
    drop(connection);
    Ok(HostPath::parse(&std::fs::canonicalize(&path)?)?)
}

#[tokio::test]
async fn an_omp_credential_view_that_never_finishes_is_given_up_on() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = view_store(
        dir.path(),
        "CREATE VIEW auth_credentials AS WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM r) SELECT x AS id, 'wanted' AS provider, 'api_key' AS credential_type, 'x' AS data FROM r WHERE x < 0;",
    )?;
    let finished = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        read_omp_host_key(&StdFileSystem::default(), &store, "wanted"),
    )
    .await
    .map_err(|_| "the read never finished")?;
    finished.unwrap_err();
    Ok(())
}
