use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, anyhow};

use super::*;

const WITHOUT_ROWID_SEED_ENV: &str = "JACKIN_BACKEND_WITHOUT_ROWID_SEED_PATH";

#[tokio::test]
async fn experimental_without_rowid_store_fails_closed_before_ready() -> Result<()> {
    if let Some(path) = std::env::var_os(WITHOUT_ROWID_SEED_ENV) {
        return seed_without_rowid_fixture(&path).await;
    }

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("experimental-without-rowid.db");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg("local_store::tests::case_09::experimental_without_rowid_store_fails_closed_before_ready")
        .arg("--nocapture")
        .env(WITHOUT_ROWID_SEED_ENV, &path);
    let output = command_output(&mut command).await?;
    anyhow::ensure!(
        output.status.success(),
        "finished-child experimental seed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let guard = acquire_store_operation(&path).await?;
    let opened = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await;
    if let Ok(store) = opened {
        store.connection().execute("BEGIN IMMEDIATE", ()).await?;
        let result = store
            .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
            .await;
        let Err(_error) = result else {
            anyhow::bail!("normal backend options must reject experimental WITHOUT ROWID");
        };
        store.connection().execute("ROLLBACK", ()).await?;
    }
    for entry in std::fs::read_dir(directory.path())? {
        anyhow::ensure!(
            !entry?.path().join("backup_ready.manifest").exists(),
            "unsupported schema must not publish backup_ready"
        );
    }
    Ok(())
}

#[tokio::test]
async fn rolled_back_schema_cannot_obtain_committed_validation() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("rolled-back.db");
    seed_old_store(&path).await?;
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema needs a backup"))?;
    store.connection().execute("ROLLBACK", ()).await?;

    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await;
    assert!(matches!(
        validation,
        Err(StoreError::CommittedSchemaMismatch)
    ));
    assert!(!has_migrated_marker(directory.path())?);
    Ok(())
}

#[tokio::test]
async fn source_change_after_validation_blocks_receipt_publication() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("changed-after-token.db");
    seed_old_store(&path).await?;
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let backup = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema needs a backup"))?;
    store
        .connection()
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '2');
             PRAGMA user_version = 2;",
        )
        .await?;
    store.connection().execute("COMMIT", ()).await?;
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    store
        .connection()
        .execute(
            "UPDATE old_rows SET payload = 'changed after validation' WHERE id = 3",
            (),
        )
        .await?;

    let marked = store.mark_migration_committed(&backup, validation).await;
    assert!(matches!(marked, Err(StoreError::CommittedSchemaMismatch)));
    assert!(
        !backup
            .directory
            .join("migration_committed.manifest")
            .exists()
    );
    Ok(())
}

async fn seed_without_rowid_fixture(path: &std::ffi::OsStr) -> Result<()> {
    let path = PathBuf::from(path);
    let path_text = path
        .to_str()
        .ok_or_else(|| anyhow!("experimental fixture path is not UTF-8"))?;
    let database = turso::Builder::new_local(path_text)
        .experimental_without_rowid(true)
        .build()
        .await?;
    let connection = database.connect()?;
    connection
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta VALUES ('schema_version', '1');
             CREATE TABLE old_rows (id INTEGER PRIMARY KEY, payload TEXT NOT NULL);
             INSERT INTO old_rows VALUES (1, 'retained');
             CREATE TABLE experimental_rows (
                store TEXT NOT NULL,
                slot INTEGER NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (store, slot)
             ) WITHOUT ROWID;
             INSERT INTO experimental_rows VALUES ('snapshot', 7, 'unsupported');
             PRAGMA user_version = 1;",
        )
        .await?;
    Ok(())
}

async fn seed_old_store(path: &Path) -> Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg("local_store::tests::seed_old_store_in_finished_child")
        .arg("--nocapture")
        .env(SEED_PATH_ENV, path);
    let output = command_output(&mut command).await?;
    anyhow::ensure!(
        output.status.success(),
        "finished-child old-schema seed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn has_migrated_marker(parent: &Path) -> Result<bool> {
    for entry in std::fs::read_dir(parent)? {
        if entry?.path().join("migration_committed.manifest").is_file() {
            return Ok(true);
        }
    }
    Ok(false)
}
