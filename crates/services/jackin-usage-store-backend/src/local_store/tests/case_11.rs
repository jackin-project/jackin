use anyhow::{Result, anyhow};

use super::*;

#[tokio::test]
async fn forensic_artifact_or_manifest_hardlink_is_rejected_before_open() -> Result<()> {
    for artifact_name in ["source.main", "preopen.manifest"] {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("live-source.db");
        let target = directory.path().join("old-target.db");
        case_10::seed_old_store(&target).await?;

        let source_text = source
            .to_str()
            .ok_or_else(|| anyhow!("source path is not UTF-8"))?;
        let owner = connect_local(source_text)
            .await
            .map_err(anyhow::Error::msg)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600))?;
        }
        owner.execute("BEGIN IMMEDIATE", ()).await?;
        let source_identity = path::resolve_source_path(&source)?.identity;

        let guard = acquire_store_operation(&target).await?;
        let store =
            LocalStore::open_managed(&target, ManagedStoreKind::QuotaSnapshot, &guard).await?;
        let evidence = store
            .evidence
            .as_ref()
            .ok_or_else(|| anyhow!("managed first-open evidence is missing"))?;
        let artifact = evidence.directory.join(artifact_name);
        std::fs::remove_file(&artifact)?;
        std::fs::hard_link(&source, &artifact)?;
        drop(forensics::take_events(&artifact));

        store.connection().execute("BEGIN IMMEDIATE", ()).await?;
        let result = store
            .backup_before_migration(ManagedStoreKind::QuotaSnapshot, Some(1), 2)
            .await;
        assert!(matches!(result, Err(StoreError::InvalidManifest)));
        assert!(!evidence.directory.join("backup_ready.manifest").exists());
        assert!(forensics::take_events(&artifact).iter().all(|event| {
            !matches!(event, forensics::CaptureEvent::ArtifactOpened(identity) if *identity == source_identity)
        }));

        case_04::ensure_child_blocked(&source).await?;
        store.connection().execute("ROLLBACK", ()).await?;
        drop(store);
        drop(guard);
        drop(owner);
        case_04::ensure_child_can_open(&source).await?;
    }
    Ok(())
}
