use super::{FailedInvocationCleanup, persist};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
};

#[test]
fn persist_returns_synced_private_patch_under_physical_common_directory() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let common_git_dir = fs::canonicalize(temporary.path())?;
    let patch = b"diff --git a/source b/source\n";

    let persisted = persist(&common_git_dir, patch)?;

    assert_eq!(fs::read(&persisted)?, patch);
    assert_eq!(
        fs::metadata(&persisted)?.permissions().mode() & 0o777,
        0o600
    );
    let invocation = persisted.parent().ok_or_else(|| {
        anyhow::anyhow!("persisted patch path does not have an invocation directory")
    })?;
    assert_eq!(
        fs::metadata(invocation)?.permissions().mode() & 0o777,
        0o700
    );
    let evidence_root = invocation
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invocation path does not have an evidence root"))?;
    assert_eq!(
        fs::metadata(evidence_root)?.permissions().mode() & 0o777,
        0o700
    );
    Ok(())
}

#[test]
fn failed_invocation_cleanup_preserves_prior_evidence() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let root = temporary.path().join("evidence");
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    let prior_evidence = root.join("prior");
    fs::create_dir(&prior_evidence)?;
    fs::set_permissions(&prior_evidence, fs::Permissions::from_mode(0o700))?;
    fs::write(prior_evidence.join("fixes.patch"), b"retain me")?;

    let invocation = root.join("failed-invocation");
    fs::create_dir(&invocation)?;
    fs::set_permissions(&invocation, fs::Permissions::from_mode(0o700))?;
    let patch = invocation.join("fixes.patch");
    fs::write(&patch, b"remove me")?;
    fs::set_permissions(&patch, fs::Permissions::from_mode(0o600))?;
    let metadata = fs::metadata(&invocation)?;
    let mut cleanup =
        FailedInvocationCleanup::new(invocation.clone(), (metadata.dev(), metadata.ino()));
    cleanup.record_patch(&fs::metadata(&patch)?);
    drop(cleanup);

    assert!(!invocation.exists());
    assert_eq!(fs::read(prior_evidence.join("fixes.patch"))?, b"retain me");
    Ok(())
}
