use super::super::process::create_private_new;
use super::{FailedInvocationCleanup, persist};
use std::{fs, os::unix::fs::PermissionsExt};

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
    let mut file = create_private_new(&path)?;
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
fn persist_preserves_evidence_beneath_a_sticky_writable_ancestor() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let ancestor = fs::canonicalize(temporary.path())?;
    let common_git_dir = ancestor.join("repository.git");
    fs::create_dir(&common_git_dir)?;
    fs::set_permissions(&common_git_dir, fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(&ancestor, fs::Permissions::from_mode(0o1777))?;
    let patch = b"private patch beneath sticky ancestor\n";

    let result = persist(
        &common_git_dir,
        patch,
        &super::SetupControls::for_test(std::time::Duration::from_secs(30)),
    );
    fs::set_permissions(&ancestor, fs::Permissions::from_mode(0o700))?;
    let persisted = result?;

    assert_eq!(fs::read(&persisted)?, patch);
    assert_eq!(
        fs::metadata(&persisted)?.permissions().mode() & 0o777,
        0o600
    );
    assert!(persisted.starts_with(&common_git_dir));
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
    let mut cleanup = FailedInvocationCleanup::new(
        super::open_private_directory(&root)?,
        std::path::PathBuf::from("failed-invocation"),
    );
    cleanup.directory_identity = Some(super::fstatat(
        &cleanup.root,
        &cleanup.name,
        super::AtFlags::AT_SYMLINK_NOFOLLOW,
    )?);
    cleanup.invocation = Some(super::open_private_directory(&invocation)?);
    cleanup.record_patch(&fs::metadata(&patch)?);
    drop(cleanup);

    assert!(!invocation.exists());
    assert_eq!(fs::read(prior_evidence.join("fixes.patch"))?, b"retain me");
    Ok(())
}

#[test]
fn retained_evidence_root_rejects_a_same_mode_path_replacement() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let parent = fs::canonicalize(temporary.path())?;
    let root = parent.join("evidence");
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    fs::write(root.join("prior.patch"), b"retain prior evidence")?;
    let root_descriptor = super::open_private_directory(&root)?;
    let retained = parent.join("retained-evidence");
    fs::rename(&root, &retained)?;
    fs::create_dir(&root)?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    fs::write(root.join("replacement-canary"), b"preserve replacement")?;

    let error = super::persist_invocation(
        &root,
        &root_descriptor,
        b"must not enter the replacement",
        &super::SetupControls::for_test(std::time::Duration::from_secs(30)),
    )
    .expect_err("a replacement evidence pathname must not be reported as the retained root");

    assert!(
        format!("{error:#}").contains("private directory path no longer names"),
        "unexpected custody error: {error:#}"
    );
    assert_eq!(
        fs::read(retained.join("prior.patch"))?,
        b"retain prior evidence"
    );
    assert_eq!(
        fs::read(root.join("replacement-canary"))?,
        b"preserve replacement"
    );
    assert_eq!(crate::fs_util::read_dir_sorted(&root)?.len(), 1);
    assert_eq!(crate::fs_util::read_dir_sorted(&retained)?.len(), 1);
    Ok(())
}
