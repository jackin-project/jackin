use std::path::Path;

use anyhow::{Result, anyhow};

use super::*;

#[tokio::test]
async fn ready_backup_hardlink_to_live_source_is_rejected_before_hash_open() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("live-source.db");
    let target = directory.path().join("old-target.db");
    case_10::seed_old_store(&target).await?;
    let owner = lock_source(&source).await?;
    let source_identity = path::resolve_source_path(&source)?.identity;

    let guard = acquire_store_operation(&target).await?;
    let store = LocalStore::open_managed(&target, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema backup was skipped"))?;
    let backup_path = store
        .evidence
        .as_ref()
        .ok_or_else(|| anyhow!("preopen evidence is missing"))?
        .directory
        .parent()
        .ok_or_else(|| anyhow!("backup directory has no parent"))?;
    let ready_directory = std::fs::read_dir(backup_path)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .find(|candidate| candidate.join("backup_ready.manifest").is_file())
        .ok_or_else(|| anyhow!("ready backup generation is missing"))?;
    let backup = ready_directory.join("rollback.sqlite");
    std::fs::remove_file(&backup)?;
    std::fs::hard_link(&source, &backup)?;
    drop(forensics::take_events(&backup));

    store
        .connection()
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '2');
             PRAGMA user_version = 2;
             COMMIT;",
        )
        .await?;
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    let result = store
        .mark_current_schema_migration_committed(ManagedStoreKind::QuotaSnapshot, 2, validation)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidManifest)));
    assert!(forensics::take_events(&backup).iter().all(|event| {
        !matches!(event, forensics::CaptureEvent::ArtifactOpened(identity) if *identity == source_identity)
    }));
    case_04::ensure_child_blocked(&source).await?;

    drop(store);
    drop(guard);
    drop(owner);
    case_04::ensure_child_can_open(&source).await?;
    Ok(())
}

#[tokio::test]
async fn ready_manifest_hardlink_to_live_source_is_skipped_before_open() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("live-source.db");
    let target = directory.path().join("old-target.db");
    case_10::seed_old_store(&target).await?;
    let owner = lock_source(&source).await?;
    let source_identity = path::resolve_source_path(&source)?.identity;

    let guard = acquire_store_operation(&target).await?;
    let store = LocalStore::open_managed(&target, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let receipt = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema backup was skipped"))?;
    let ready = receipt.directory.join("backup_ready.manifest");
    std::fs::remove_file(&ready)?;
    std::fs::hard_link(&source, &ready)?;
    drop(forensics::take_events(&ready));

    store
        .connection()
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '2');
             PRAGMA user_version = 2;
             COMMIT;",
        )
        .await?;
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    let result = store
        .mark_current_schema_migration_committed(ManagedStoreKind::QuotaSnapshot, 2, validation)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidManifest)));
    assert!(forensics::take_events(&ready).iter().all(|event| {
        !matches!(event, forensics::CaptureEvent::ArtifactOpened(identity) if *identity == source_identity)
    }));
    case_04::ensure_child_blocked(&source).await?;

    drop(store);
    drop(guard);
    drop(owner);
    case_04::ensure_child_can_open(&source).await?;
    Ok(())
}

#[tokio::test]
async fn migrated_manifest_hardlink_to_live_source_is_rejected_before_open() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let source = directory.path().join("live-source.db");
    let target = directory.path().join("old-target.db");
    case_10::seed_old_store(&target).await?;
    let owner = lock_source(&source).await?;
    let source_identity = path::resolve_source_path(&source)?.identity;

    let guard = acquire_store_operation(&target).await?;
    let store = LocalStore::open_managed(&target, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    store.connection().execute("BEGIN IMMEDIATE", ()).await?;
    let receipt = store
        .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
        .await?
        .ok_or_else(|| anyhow!("old schema backup was skipped"))?;
    store
        .connection()
        .execute_batch(
            "CREATE TABLE _meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO _meta (key, value) VALUES ('schema_version', '2');
             PRAGMA user_version = 2;
             COMMIT;",
        )
        .await?;
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    store.mark_migration_committed(&receipt, validation).await?;

    let marker = receipt.directory.join("migration_committed.manifest");
    std::fs::remove_file(&marker)?;
    std::fs::hard_link(&source, &marker)?;
    drop(forensics::take_events(&marker));
    let validation = store
        .validate_committed_migration(ManagedStoreKind::QuotaSnapshot, 2)
        .await?;
    let result = store
        .mark_current_schema_migration_committed(ManagedStoreKind::QuotaSnapshot, 2, validation)
        .await;
    assert!(matches!(result, Err(StoreError::InvalidManifest)));
    assert!(forensics::take_events(&marker).iter().all(|event| {
        !matches!(event, forensics::CaptureEvent::ArtifactOpened(identity) if *identity == source_identity)
    }));
    case_04::ensure_child_blocked(&source).await?;

    drop(store);
    drop(guard);
    drop(owner);
    case_04::ensure_child_can_open(&source).await?;
    Ok(())
}

async fn lock_source(path: &Path) -> Result<Connection> {
    let path_text = path
        .to_str()
        .ok_or_else(|| anyhow!("source path is not UTF-8"))?;
    let connection = connect_local(path_text).await.map_err(anyhow::Error::msg)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    connection.execute("BEGIN IMMEDIATE", ()).await?;
    Ok(connection)
}
