use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, anyhow};

use super::*;

const LOCK_CHILD_PATH_ENV: &str = "JACKIN_BACKEND_LOCK_CHILD_PATH";
const LOCK_CHILD_MODE_ENV: &str = "JACKIN_BACKEND_LOCK_CHILD_MODE";

#[tokio::test]
async fn default_process_lock_survives_backup_and_owner_lifetime() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cross-process.db");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg("local_store::tests::seed_old_store_in_finished_child")
        .arg("--nocapture")
        .env(SEED_PATH_ENV, &path);
    let output = command_output(&mut command).await?;
    anyhow::ensure!(
        output.status.success(),
        "finished-child old-schema seed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let guard = acquire_store_operation(&path).await?;
    let store = LocalStore::open_managed(&path, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    ensure_child_blocked(&path).await?;

    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    ensure_child_blocked(&path).await?;
    let backup = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema needs a logical backup"))?;
    ensure_child_blocked(&path).await?;

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
    store.mark_migration_committed(&backup, validation).await?;
    ensure_child_blocked(&path).await?;

    store
        .connection()
        .execute(
            "UPDATE old_rows SET payload = 'still owned' WHERE id = 3",
            (),
        )
        .await?;
    let mut rows = store
        .connection()
        .query("SELECT payload FROM old_rows WHERE id = 3", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or_else(|| anyhow!("ordinary cached read found no row"))?;
    anyhow::ensure!(row.get::<String>(0)? == "still owned");
    drop(row);
    drop(rows);
    ensure_child_blocked(&path).await?;

    drop(store);
    drop(guard);
    ensure_child_can_open(&path).await?;
    Ok(())
}

#[tokio::test]
async fn child_lock_attempt() -> Result<()> {
    if std::env::var_os(LOCK_CHILD_PATH_ENV).is_none() {
        return Ok(());
    }
    run_child_lock_attempt().await
}

async fn run_child_lock_attempt() -> Result<()> {
    let path = std::env::var_os(LOCK_CHILD_PATH_ENV)
        .map(PathBuf::from)
        .context("child lock path is present")?;
    let path_text = path.to_str().context("child lock path is UTF-8")?;
    let attempt = async {
        let database = turso::Builder::new_local(path_text).build().await?;
        let connection = database.connect()?;
        connection.execute("BEGIN IMMEDIATE", ()).await?;
        connection.execute("ROLLBACK", ()).await?;
        anyhow::Ok(())
    }
    .await;
    match std::env::var(LOCK_CHILD_MODE_ENV).as_deref() {
        Ok("blocked") => {
            anyhow::ensure!(
                attempt.is_err(),
                "a second process entered the default local store lock"
            );
            Ok(())
        }
        Ok("open") => {
            attempt?;
            Ok(())
        }
        _ => Err(anyhow!("child lock mode is invalid")),
    }
}

pub(super) async fn ensure_child_blocked(path: &Path) -> Result<()> {
    run_lock_child(path, "blocked").await
}

pub(super) async fn ensure_child_can_open(path: &Path) -> Result<()> {
    run_lock_child(path, "open").await
}

async fn run_lock_child(path: &Path, mode: &str) -> Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--exact")
        .arg("local_store::tests::case_04::child_lock_attempt")
        .arg("--nocapture")
        .env(LOCK_CHILD_PATH_ENV, path)
        .env(LOCK_CHILD_MODE_ENV, mode)
        .env_remove("LIMBO_DISABLE_FILE_LOCK");
    let output = command_output(&mut command).await?;
    anyhow::ensure!(
        output.status.success(),
        "child lock check ({mode}) failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
