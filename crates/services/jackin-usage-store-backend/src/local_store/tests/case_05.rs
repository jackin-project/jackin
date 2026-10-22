use anyhow::{Result, anyhow};

use super::*;
use crate::connect_local;

#[tokio::test]
async fn opened_inode_cannot_regain_capture_authority_after_rename_and_unlink() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let original = directory.path().join("original.db");
    let alias = directory.path().join("remaining-link.db");
    let original_text = original
        .to_str()
        .ok_or_else(|| anyhow!("source path is not UTF-8"))?;
    let legacy = connect_local(original_text)
        .await
        .map_err(anyhow::Error::msg)?;
    std::fs::hard_link(&original, &alias)?;
    std::fs::remove_file(&original)?;

    let guard = acquire_store_operation(&alias).await?;
    let store = LocalStore::open_managed(&alias, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    assert_eq!(store.preopen_status(), PreopenStatus::NotCaptured);
    let events = forensics::take_events(&alias);
    assert!(!events.contains(&forensics::CaptureEvent::SourceOpened));
    assert!(
        std::fs::read_dir(directory.path())?
            .filter_map(std::result::Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".jackin-quota-backup-"))
    );

    drop(store);
    drop(guard);
    drop(legacy);
    Ok(())
}

#[tokio::test]
async fn plain_rename_keeps_opened_inode_capture_forbidden_with_live_rows() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let original = directory.path().join("before-rename.db");
    let renamed = directory.path().join("after-rename.db");
    let original_text = original
        .to_str()
        .ok_or_else(|| anyhow!("source path is not UTF-8"))?;
    let legacy = connect_local(original_text)
        .await
        .map_err(anyhow::Error::msg)?;
    let mut retained_rows = legacy.query("SELECT 1", ()).await?;
    std::fs::rename(&original, &renamed)?;

    let guard = acquire_store_operation(&renamed).await?;
    let store = LocalStore::open_managed(&renamed, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    assert_eq!(store.preopen_status(), PreopenStatus::NotCaptured);
    assert!(!forensics::take_events(&renamed).contains(&forensics::CaptureEvent::SourceOpened));
    assert!(retained_rows.next().await?.is_some());

    drop(store);
    drop(guard);
    drop(retained_rows);
    drop(legacy);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn canonical_parent_alias_reuses_legacy_opened_evidence_state() -> Result<()> {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir()?;
    let actual_parent = directory.path().join("actual");
    let alias_parent = directory.path().join("parent-alias");
    std::fs::create_dir(&actual_parent)?;
    symlink(&actual_parent, &alias_parent)?;
    let original = actual_parent.join("usage.db");
    let alias = alias_parent.join("usage.db");
    let original_text = original
        .to_str()
        .ok_or_else(|| anyhow!("source path is not UTF-8"))?;
    let legacy = connect_local(original_text)
        .await
        .map_err(anyhow::Error::msg)?;

    let guard = acquire_store_operation(&alias).await?;
    let store = LocalStore::open_managed(&alias, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    assert_eq!(store.preopen_status(), PreopenStatus::NotCaptured);
    assert!(!forensics::take_events(&alias).contains(&forensics::CaptureEvent::SourceOpened));
    drop(store);
    drop(guard);
    drop(legacy);
    Ok(())
}

#[tokio::test]
async fn case_alias_is_unified_when_the_filesystem_resolves_it_to_same_inode() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let original = directory.path().join("MixedCase.db");
    let alias = directory.path().join("mixedcase.db");
    let original_text = original
        .to_str()
        .ok_or_else(|| anyhow!("source path is not UTF-8"))?;
    let legacy = connect_local(original_text)
        .await
        .map_err(anyhow::Error::msg)?;
    let original_identity = path::resolve_source_path(&original)?.identity;
    let alias_identity = path::resolve_source_path(&alias)?.identity;
    if original_identity.is_none() || alias_identity != original_identity {
        drop(legacy);
        return Ok(());
    }

    let guard = acquire_store_operation(&alias).await?;
    let store = LocalStore::open_managed(&alias, ManagedStoreKind::QuotaSnapshot, &guard).await?;
    assert_eq!(store.preopen_status(), PreopenStatus::NotCaptured);
    assert!(!forensics::take_events(&alias).contains(&forensics::CaptureEvent::SourceOpened));
    drop(store);
    drop(guard);
    drop(legacy);
    Ok(())
}
