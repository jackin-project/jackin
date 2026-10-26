use super::{FailedInvocationCleanup, persist};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
};

#[test]
fn concurrent_first_use_preserves_every_private_patch() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let common_git_dir = fs::canonicalize(temporary.path())?;
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
    let paths = std::thread::scope(|scope| {
        let handles = (0..16)
            .map(|number| {
                let barrier = std::sync::Arc::clone(&barrier);
                let common_git_dir = &common_git_dir;
                scope.spawn(move || {
                    let patch = format!("private patch {number}\n");
                    let controls =
                        super::SetupControls::for_test(std::time::Duration::from_secs(30));
                    barrier.wait();
                    let path = persist(common_git_dir, patch.as_bytes(), &controls)?;
                    anyhow::ensure!(fs::read(&path)? == patch.as_bytes(), "patch changed");
                    Ok::<_, anyhow::Error>(path)
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("evidence worker"))
            .collect::<anyhow::Result<Vec<_>>>()
    })?;
    assert_eq!(
        paths
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        16
    );
    Ok(())
}

#[test]
fn cancelled_recovery_does_not_create_evidence() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let common_git_dir = fs::canonicalize(temporary.path())?;
    let controls = super::SetupControls::new(
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        std::time::Duration::from_secs(30),
    );
    persist(&common_git_dir, b"retained in snapshot", &controls).unwrap_err();
    assert!(!common_git_dir.join("jackin-hook-evidence").exists());
    Ok(())
}

#[test]
fn expired_recovery_does_not_write_patch_bytes() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("fixes.patch");
    let mut file = super::create_private_new(&path)?;
    let controls = super::SetupControls::for_test(std::time::Duration::ZERO);
    super::write_controlled(&mut file, &vec![1; 128 * 1024], &controls).unwrap_err();
    assert_eq!(fs::metadata(path)?.len(), 0);
    Ok(())
}

#[test]
fn persist_returns_synced_private_patch_under_physical_common_directory() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let common_git_dir = fs::canonicalize(temporary.path())?;
    let patch = b"diff --git a/source b/source\n";

    let persisted = persist(
        &common_git_dir,
        patch,
        &super::SetupControls::for_test(std::time::Duration::from_secs(30)),
    )?;

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
