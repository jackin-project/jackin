use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, anyhow};

use super::*;

#[tokio::test]
async fn changed_forensic_artifact_prevents_ready_before_any_migration() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("changed-artifact.db");
    seed_old_store(&path).await?;
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    let forensic = forensic_directory(&path)?;
    let main_copy = forensic.join("source.main");
    let mut bytes = std::fs::read(&main_copy)?;
    let first = bytes.first_mut().context("forensic main has bytes")?;
    *first ^= 0x80;
    std::fs::write(&main_copy, bytes)?;

    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let result = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidManifest)));
    assert_no_ready(directory.path())?;
    assert_old_version_and_rows(store.connection()).await?;
    store.connection().execute("ROLLBACK", ()).await?;
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_forensic_artifact_prevents_ready_without_following_source() -> Result<()> {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir()?;
    let path = directory.path().join("symlink-artifact.db");
    seed_old_store(&path).await?;
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    let forensic = forensic_directory(&path)?;
    let main_copy = forensic.join("source.main");
    std::fs::remove_file(&main_copy)?;
    symlink(&path, &main_copy)?;

    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let result = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidManifest)));
    assert_no_ready(directory.path())?;
    assert_old_version_and_rows(store.connection()).await?;
    store.connection().execute("ROLLBACK", ()).await?;
    Ok(())
}

#[tokio::test]
async fn missing_forensic_artifact_prevents_ready() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("missing-artifact.db");
    seed_old_store(&path).await?;
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    std::fs::remove_file(forensic_directory(&path)?.join("source.main"))?;

    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let result = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await;
    let Err(_error) = result else {
        anyhow::bail!("missing forensic artifact must prevent backup_ready");
    };
    assert_no_ready(directory.path())?;
    assert_old_version_and_rows(store.connection()).await?;
    store.connection().execute("ROLLBACK", ()).await?;
    Ok(())
}

#[tokio::test]
async fn artifact_corruption_after_ready_blocks_migration_receipt_recovery() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("ready-artifact.db");
    seed_old_store(&path).await?;
    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let receipt = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema needs a logical backup"))?;
    store
        .connection()
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '2');
             PRAGMA user_version = 2;",
        )
        .await?;
    store.connection().execute("COMMIT", ()).await?;

    let forensic = forensic_directory(&path)?;
    let main_copy = forensic.join("source.main");
    let mut bytes = std::fs::read(&main_copy)?;
    let first = bytes.first_mut().context("forensic main has bytes")?;
    *first ^= 0x40;
    std::fs::write(main_copy, bytes)?;
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    let marked = store.mark_migration_committed(&receipt, validation).await;
    assert!(matches!(marked, Err(StoreError::InvalidManifest)));
    assert!(
        !receipt
            .directory
            .join("migration_committed.manifest")
            .exists()
    );
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

fn forensic_directory(source: &Path) -> Result<PathBuf> {
    std::fs::read_dir(source.parent().context("source has parent")?)?
        .filter_map(std::result::Result::ok)
        .find(|entry| entry.path().join("preopen.manifest").is_file())
        .map(|entry| entry.path())
        .ok_or_else(|| anyhow!("pre-open forensic directory is missing"))
}

fn assert_no_ready(parent: &Path) -> Result<()> {
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        anyhow::ensure!(
            !entry.path().join("backup_ready.manifest").exists(),
            "invalid forensic evidence must not publish backup_ready"
        );
    }
    Ok(())
}

async fn assert_old_version_and_rows(connection: &Connection) -> Result<()> {
    let mut version = connection.query("PRAGMA user_version", ()).await?;
    assert_eq!(
        version
            .next()
            .await?
            .context("old schema version")?
            .get::<i64>(0)?,
        1
    );
    drop(version);
    let mut rows = connection
        .query("SELECT COUNT(*) FROM old_rows", ())
        .await?;
    assert_eq!(
        rows.next()
            .await?
            .context("old data count")?
            .get::<i64>(0)?,
        2
    );
    drop(rows);
    Ok(())
}
