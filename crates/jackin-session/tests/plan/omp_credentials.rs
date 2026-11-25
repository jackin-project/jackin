// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{FileMode, FileSystem, HostPath, StdFileSystem};
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

const HANG: &str = "CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, data TEXT);
     WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM r WHERE x < 400000) INSERT INTO auth_credentials (id, provider, data) SELECT x, 'wanted', 'x' FROM r;
     ALTER TABLE auth_credentials ADD COLUMN credential_type TEXT AS (CASE WHEN length(hex(zeroblob(60000 + id % 2))) > 0 THEN 'never' END);";

#[tokio::test]
async fn an_omp_credential_query_that_never_finishes_is_given_up_on_and_frees_its_place()
-> TestResult {
    let fs = StdFileSystem::default();
    for _ in 0..3 {
        let dir = tempfile::tempdir()?;
        let store = view_store(dir.path(), HANG)?;
        let finished = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            read_omp_host_key(&fs, &store, "wanted"),
        )
        .await
        .map_err(|_| "the read never finished")?;
        finished.unwrap_err();
    }
    let dir = tempfile::tempdir()?;
    let store = view_store(
        dir.path(),
        "CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, credential_type TEXT, data TEXT);
         INSERT INTO auth_credentials VALUES (1, 'wanted', 'api_key', '{\"key\": \"fake-wanted-key\"}');",
    )?;
    let key = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read_omp_host_key(&fs, &store, "wanted"),
    )
    .await
    .map_err(|_| "the worker of an abandoned query still holds its place")??;
    assert_eq!(key.expose(), "fake-wanted-key");
    Ok(())
}

#[tokio::test]
async fn an_omp_credential_view_is_refused_instead_of_run() -> TestResult {
    for schema in [
        "CREATE VIEW auth_credentials AS WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM r) SELECT x AS id, 'wanted' AS provider, 'api_key' AS credential_type, 'x' AS data FROM r WHERE x < 0;",
        "CREATE TABLE real (id INTEGER PRIMARY KEY, provider TEXT, credential_type TEXT, data TEXT);
         INSERT INTO real VALUES (1, 'wanted', 'api_key', '{\"key\": \"fake-wanted-key\"}');
         CREATE VIEW auth_credentials AS SELECT * FROM real;",
        "CREATE VIEW auth_credentials AS SELECT 1 AS id, 'wanted' AS provider, 'api_key' AS credential_type, '{\"key\": \"fake-wanted-key\"}' AS data;",
        "CREATE VIEW auth_credentials AS SELECT DISTINCT a.column1 AS id, 'wanted' AS provider, 'api_key' AS credential_type, 'x' AS data FROM (VALUES (1),(2),(3)) a, (VALUES (1),(2),(3)) b;",
    ] {
        let dir = tempfile::tempdir()?;
        let store = view_store(dir.path(), schema)?;
        let started = std::time::Instant::now();
        read_omp_host_key(&StdFileSystem::default(), &store, "wanted")
            .await
            .unwrap_err();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(4),
            "the view was run"
        );
    }
    Ok(())
}

#[tokio::test]
async fn an_omp_value_that_a_query_builds_beyond_the_length_limit_is_refused() -> TestResult {
    let table = |pad: u32| {
        format!(
            "CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, credential_type TEXT, data TEXT AS (substr('{{\"key\": \"k\"}}' || printf('%.*c', {pad}, 'x'), 1, 12)));
             INSERT INTO auth_credentials (id, provider, credential_type) VALUES (1, 'wanted', 'api_key');"
        )
    };
    let dir = tempfile::tempdir()?;
    let store = view_store(dir.path(), &table(300_000))?;
    read_omp_host_key(&StdFileSystem::default(), &store, "wanted")
        .await
        .unwrap_err();
    let small = tempfile::tempdir()?;
    let store = view_store(small.path(), &table(30))?;
    let key = read_omp_host_key(&StdFileSystem::default(), &store, "wanted").await?;
    assert_eq!(key.expose(), "k");
    Ok(())
}

fn key_store(dir: &Path, total: usize) -> Result<HostPath, Box<dyn std::error::Error>> {
    let frame = "{\"key\": \"\"}".len();
    let data = format!("{{\"key\": \"{}\"}}", "k".repeat(total - frame));
    assert_eq!(data.len(), total);
    view_store(
        dir,
        &format!(
            "CREATE TABLE auth_credentials (id INTEGER PRIMARY KEY, provider TEXT, credential_type TEXT, data TEXT);
             INSERT INTO auth_credentials (provider, credential_type, data) VALUES ('wanted', 'api_key', '{data}');"
        ),
    )
}

#[tokio::test]
async fn an_omp_value_of_exactly_the_cap_resolves_and_one_byte_more_is_unreadable() -> TestResult {
    let cap = jackin_core::STORE_VALUE_MAX_BYTES;
    let fs = StdFileSystem::default();
    let exact = tempfile::tempdir()?;
    let key = read_omp_host_key(&fs, &key_store(exact.path(), cap)?, "wanted").await?;
    assert_eq!(key.expose().len(), cap - "{\"key\": \"\"}".len());
    let over = tempfile::tempdir()?;
    read_omp_host_key(&fs, &key_store(over.path(), cap + 1)?, "wanted")
        .await
        .unwrap_err();
    Ok(())
}
