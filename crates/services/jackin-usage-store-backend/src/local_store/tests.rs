use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::Sender;

use anyhow::{Result, anyhow};

use super::*;
use crate::connect_local;
mod support;
pub(super) use support::command_output;

const SEED_PATH_ENV: &str = "JACKIN_BACKEND_BACKUP_SEED_PATH";

pub(super) struct TestPauseRelease(Option<Sender<()>>);

impl TestPauseRelease {
    pub(super) fn new(sender: Sender<()>) -> Self {
        Self(Some(sender))
    }
}

impl Drop for TestPauseRelease {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _send_result = sender.send(());
        }
    }
}

mod case_04;
mod case_05;
mod case_06;
mod case_07;
mod case_08;
mod case_09;
mod case_10;
mod case_11;
mod case_12;
mod case_13;

#[tokio::test]
async fn fresh_managed_open_records_absent_preopen_evidence() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("fresh.db");
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::CliUsage, &guard).await?;

    assert_eq!(store.preopen_status(), PreopenStatus::CapturedAbsent);
    assert!(path.is_file());
    Ok(())
}

#[tokio::test]
async fn committed_rows_after_absent_capture_are_in_the_logical_backup() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("late-old-store.db");
    let (ready_receiver, release_sender) = registry::set_managed_prepare_pause(&path)?;
    let release = TestPauseRelease::new(release_sender);
    let guard = acquire_store_operation(&path).await?;
    let open_path = path.clone();
    let opening = tokio::spawn(async move {
        LocalStore::open_managed(&open_path, ManagedStoreKind::QuotaSnapshot, &guard).await
    });

    tokio::task::spawn_blocking(move || {
        ready_receiver.recv_timeout(std::time::Duration::from_secs(5))
    })
    .await??;
    let seed_database =
        turso::Builder::new_local(path.to_str().ok_or_else(|| anyhow!("path is not UTF-8"))?)
            .build()
            .await?;
    let seed = seed_database.connect()?;
    seed.execute_batch(
        "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO _meta (key, value) VALUES ('schema_version', '1');
         CREATE TABLE old_rows (id INTEGER PRIMARY KEY, payload TEXT NOT NULL);
         INSERT INTO old_rows (id, payload) VALUES (7, 'committed after absent capture');
         PRAGMA user_version = 1;",
    )
    .await?;
    drop(seed);
    drop(seed_database);

    drop(release);
    registry::clear_managed_prepare_pause(&path);
    let store = tokio::time::timeout(std::time::Duration::from_secs(10), opening)
        .await
        .map_err(|_| anyhow!("managed open did not finish after prepare pause release"))???;
    assert_eq!(store.preopen_status(), PreopenStatus::CapturedAbsent);

    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let receipt = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema after absent capture needs a logical backup"))?;
    assert!(receipt.logical_digest().rows >= 2);
    assert!(receipt.logical_digest().schema_objects >= 2);
    let backup = receipt.directory.join("rollback.sqlite");
    let backup_text = backup
        .to_str()
        .ok_or_else(|| anyhow!("backup path is not UTF-8"))?;
    let restored_database = turso::Builder::new_local(backup_text).build().await?;
    let restored = restored_database.connect()?;
    let mut rows = restored
        .query("SELECT id, payload FROM old_rows WHERE id = 7", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow!("backup row is missing"))?;
    assert_eq!(row.get::<i64>(0)?, 7);
    assert_eq!(row.get::<String>(1)?, "committed after absent capture");
    drop(row);
    assert!(rows.next().await?.is_none());
    drop(rows);
    drop(restored);
    drop(restored_database);

    store
        .connection()
        .execute(
            "UPDATE _meta SET value = '2' WHERE key = 'schema_version'",
            (),
        )
        .await?;
    store
        .connection()
        .execute("PRAGMA user_version = 2", ())
        .await?;
    store.connection().execute("COMMIT", ()).await?;
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    let migrated = store.mark_migration_committed(&receipt, validation).await?;
    assert_eq!(migrated.new_version(), 2);
    assert_eq!(migrated.generation(), receipt.generation());

    let ready =
        manifest::read_manifest_on_worker(&receipt.directory.join("backup_ready.manifest")).await?;
    assert_eq!(
        ready.get("forensic_status").map(String::as_str),
        Some("preopen_noncoherent")
    );
    assert_eq!(
        ready.get("forensic_source_device").map(String::as_str),
        Some("absent")
    );
    Ok(())
}

#[tokio::test]
async fn canceled_managed_open_releases_its_reservation_after_preopen_work_finishes() -> Result<()>
{
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("canceled-open.db");
    let (ready_receiver, release_sender) = registry::set_managed_prepare_pause(&path)?;
    let release = TestPauseRelease::new(release_sender);
    let first_guard = acquire_store_operation(&path).await?;
    let first_path = path.clone();
    let first_open = tokio::spawn(async move {
        LocalStore::open_managed(&first_path, ManagedStoreKind::QuotaSnapshot, &first_guard).await
    });

    tokio::task::spawn_blocking(move || {
        ready_receiver.recv_timeout(std::time::Duration::from_secs(5))
    })
    .await??;
    first_open.abort();
    let first_result = tokio::time::timeout(std::time::Duration::from_secs(5), first_open)
        .await
        .map_err(|_| anyhow!("canceled managed open task did not finish"))?;
    match first_result {
        Err(error) if error.is_cancelled() => {}
        _ => return Err(anyhow!("managed open was not canceled at its test barrier")),
    }

    drop(release);
    registry::clear_managed_prepare_pause(&path);

    let guard = acquire_store_operation(&path).await?;
    let store = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard),
    )
    .await??;
    assert_eq!(store.preopen_status(), PreopenStatus::CapturedAbsent);
    Ok(())
}

#[tokio::test]
async fn legacy_open_prevents_late_capture_but_allows_current_store_owner() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("legacy-open.db");
    let path_text = path.to_string_lossy();
    let legacy_connection = connect_local(&path_text)
        .await
        .map_err(anyhow::Error::msg)?;
    drop(legacy_connection);

    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    assert_eq!(store.preopen_status(), PreopenStatus::NotCaptured);

    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let result = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await;
    assert!(matches!(
        result,
        Err(StoreError::MigrationRequiresProcessRestart)
    ));
    store.connection().execute("ROLLBACK", ()).await?;
    Ok(())
}

#[tokio::test]
async fn each_backup_attempt_gets_a_new_ready_generation_for_the_current_preimage() -> Result<()> {
    if std::env::var_os(SEED_PATH_ENV).is_some() {
        return Ok(());
    }
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("old-store.db");
    let status = Command::new(std::env::current_exe()?)
        .arg("--exact")
        .arg("local_store::tests::seed_old_store_in_finished_child")
        .arg("--nocapture")
        .env(SEED_PATH_ENV, &path)
        .status()?;
    assert!(status.success());

    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    assert_eq!(store.preopen_status(), PreopenStatus::CapturedExisting);

    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let first = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("first backup was unexpectedly skipped"))?;
    store.connection().execute("ROLLBACK", ()).await?;

    store
        .connection()
        .execute("UPDATE old_rows SET payload = 'repaired' WHERE id = 3", ())
        .await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let second = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("retry backup was unexpectedly skipped"))?;

    assert_ne!(first.generation(), second.generation());
    assert_ne!(
        first.logical_digest().data_sha256,
        second.logical_digest().data_sha256
    );
    let generation_dirs = std::fs::read_dir(directory.path())?
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| entry.path().join("backup_ready.manifest").is_file())
        .collect::<Vec<_>>();
    assert_eq!(generation_dirs.len(), 2);

    store
        .connection()
        .execute("PRAGMA user_version = 2", ())
        .await?;
    store
        .connection()
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '2');",
        )
        .await?;
    store.connection().execute("COMMIT", ()).await?;

    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    let recovered = store
        .mark_current_schema_migration_committed(ManagedStoreKind::QuotaSnapshot, 2, validation)
        .await?
        .ok_or_else(|| anyhow!("current schema receipt was not recovered"))?;
    assert_eq!(recovered.new_version(), 2);
    assert_eq!(recovered.generation(), second.generation());
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    assert!(store
        .mark_current_schema_migration_committed(
            ManagedStoreKind::QuotaSnapshot,
            2,
            validation,
        )
        .await?
        .is_none());

    let mut first_directory = None;
    let mut second_directory = None;
    for entry in &generation_dirs {
        let ready =
            manifest::read_manifest_on_worker(&entry.path().join("backup_ready.manifest")).await?;
        match ready.get("generation").map(String::as_str) {
            Some(generation) if generation == first.generation() => {
                first_directory = Some(entry.path());
            }
            Some(generation) if generation == second.generation() => {
                second_directory = Some(entry.path());
            }
            _ => {}
        }
    }
    let first_directory =
        first_directory.ok_or_else(|| anyhow!("first ready generation was not found"))?;
    let second_directory =
        second_directory.ok_or_else(|| anyhow!("second ready generation was not found"))?;
    assert!(
        !first_directory
            .join("migration_committed.manifest")
            .exists()
    );
    assert!(
        second_directory
            .join("migration_committed.manifest")
            .is_file()
    );
    Ok(())
}

#[tokio::test]
async fn seed_old_store_in_finished_child() -> Result<()> {
    let Some(path) = std::env::var_os(SEED_PATH_ENV) else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let path = path
        .to_str()
        .ok_or_else(|| anyhow!("seed database path is not valid Unicode"))?;
    let connection = connect_local(path).await.map_err(anyhow::Error::msg)?;
    connection
        .execute(
            "CREATE TABLE old_rows (id INTEGER PRIMARY KEY AUTOINCREMENT, payload TEXT, score REAL, raw BLOB, nullable TEXT)",
            (),
        )
        .await?;
    connection
        .execute("CREATE INDEX old_rows_payload ON old_rows(payload)", ())
        .await?;
    connection
        .execute(
            "CREATE VIEW old_rows_view AS SELECT id, payload FROM old_rows",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO old_rows(id, payload, score, raw, nullable) VALUES (1, 'first', 1.25, X'00FF', NULL), (3, 'third', -0.0, X'107F', 'kept')",
            (),
        )
        .await?;
    connection
        .execute(
            "INSERT INTO old_rows(id, payload) VALUES (20, 'sequence')",
            (),
        )
        .await?;
    connection
        .execute("DELETE FROM old_rows WHERE id = 20", ())
        .await?;
    connection.execute("PRAGMA user_version = 1", ()).await?;
    Ok(())
}
