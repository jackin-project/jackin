use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow};

use super::*;

#[tokio::test]
async fn live_main_inode_renamed_to_wal_or_journal_is_rejected_before_capture() -> Result<()> {
    for suffix in ["-wal", "-journal"] {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("live-source.db");
        let target = directory.path().join("old-target.db");
        seed_old_store(&target).await?;

        let source_text = source
            .to_str()
            .ok_or_else(|| anyhow!("source path is not UTF-8"))?;
        let owner = connect_local(source_text)
            .await
            .map_err(anyhow::Error::msg)?;
        owner.execute("BEGIN IMMEDIATE", ()).await?;
        let sidecar = path::sidecar_path(&target, suffix);
        std::fs::rename(&source, &sidecar)?;

        let guard = acquire_store_operation(&target).await?;
        let result =
            LocalStore::open_managed(&target, ManagedStoreKind::QuotaSnapshot, &guard).await;
        assert!(matches!(
            result,
            Err(StoreError::SourceIdentityAlreadyOpened)
        ));
        assert!(!forensics::take_events(&target).contains(&forensics::CaptureEvent::SourceOpened));
        drop(guard);

        case_04::ensure_child_blocked(&sidecar).await?;
        drop(owner);
        case_04::ensure_child_can_open(&sidecar).await?;
    }
    Ok(())
}

pub(super) async fn seed_old_store(path: &Path) -> Result<()> {
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
