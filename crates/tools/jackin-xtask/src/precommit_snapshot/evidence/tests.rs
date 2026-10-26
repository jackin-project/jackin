use super::super::process::create_private_new;
use super::{FailedInvocationCleanup, persist};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    process::Command,
};

const EVIDENCE_UMASK_CHILD: &str = "JACKIN_EVIDENCE_UMASK_CHILD";
const EVIDENCE_UMASK_TEST: &str =
    "precommit_snapshot::evidence::tests::persist_normalizes_only_new_evidence_under_umask_0777";

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
fn common_directory_sync_uses_the_retained_parent_descriptor() -> anyhow::Result<()> {
    let temporary = tempfile::tempdir()?;
    let common_git_dir = temporary.path().join("common.git");
    fs::create_dir(&common_git_dir)?;
    fs::set_permissions(&common_git_dir, fs::Permissions::from_mode(0o700))?;
    let common_git_dir = fs::canonicalize(common_git_dir)?;
    let displaced_original = temporary.path().join("original-common-directory");
    let displaced_replacement = temporary.path().join("replacement-common-directory");
    let patch = b"original common directory patch bytes\n";
    super::install_common_directory_sync_test(super::CommonDirectorySyncTest {
        common_directory: common_git_dir.clone(),
        displaced_original: displaced_original.clone(),
        displaced_replacement: displaced_replacement.clone(),
        original_identity: None,
        original_mode: None,
        replacement_identity: None,
        selected_sync_identity: None,
    })?;

    let persisted = persist(
        &common_git_dir,
        patch,
        &super::SetupControls::for_test(std::time::Duration::from_secs(30)),
    );
    let test = super::take_common_directory_sync_test()?;
    let persisted = persisted?;
    let original_identity = test
        .original_identity
        .ok_or_else(|| anyhow::anyhow!("original common directory identity was not recorded"))?;
    let replacement_identity = test
        .replacement_identity
        .ok_or_else(|| anyhow::anyhow!("replacement common directory identity was not recorded"))?;
    let selected_sync_identity = test
        .selected_sync_identity
        .ok_or_else(|| anyhow::anyhow!("selected sync descriptor identity was not recorded"))?;

    assert_eq!(selected_sync_identity, original_identity);
    assert_ne!(selected_sync_identity, replacement_identity);
    assert_eq!(
        (
            fs::metadata(&common_git_dir)?.dev(),
            fs::metadata(&common_git_dir)?.ino()
        ),
        original_identity,
        "original common directory is restored before later custody checks"
    );
    assert_eq!(fs::read(&persisted)?, patch);
    assert_eq!(
        fs::read(displaced_replacement.join("replacement-canary"))?,
        b"preserve replacement common directory canary\n"
    );
    assert_eq!(
        (
            fs::metadata(&displaced_replacement)?.dev(),
            fs::metadata(&displaced_replacement)?.ino()
        ),
        replacement_identity
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
fn persist_normalizes_only_new_evidence_under_umask_0777() -> anyhow::Result<()> {
    if std::env::var_os(EVIDENCE_UMASK_CHILD).is_some() {
        let fresh = std::path::PathBuf::from(
            std::env::var_os("JACKIN_EVIDENCE_FRESH_GIT_DIR")
                .ok_or_else(|| anyhow::anyhow!("fresh Git directory is missing"))?,
        );
        let unsafe_existing = std::path::PathBuf::from(
            std::env::var_os("JACKIN_EVIDENCE_UNSAFE_GIT_DIR")
                .ok_or_else(|| anyhow::anyhow!("unsafe Git directory is missing"))?,
        );
        nix::sys::stat::umask(nix::sys::stat::Mode::from_bits_truncate(0o777));
        let controls = || super::SetupControls::for_test(std::time::Duration::from_secs(30));
        let first = persist(&fresh, b"first private patch\n", &controls())?;
        let root = first
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or_else(|| anyhow::anyhow!("evidence root path is missing"))?;
        let root_before = fs::symlink_metadata(root)?;
        assert_eq!(root_before.permissions().mode() & 0o777, 0o700);
        assert_eq!(fs::metadata(&first)?.permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read(&first)?, b"first private patch\n");
        let second = persist(&fresh, b"second private patch\n", &controls())?;
        let root_after = fs::symlink_metadata(root)?;
        assert_eq!(
            (root_before.dev(), root_before.ino()),
            (root_after.dev(), root_after.ino())
        );
        assert_eq!(fs::metadata(&second)?.permissions().mode() & 0o777, 0o600);
        let invocation = second
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invocation path is missing"))?;
        assert_eq!(
            fs::metadata(invocation)?.permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(fs::read(second)?, b"second private patch\n");

        let unsafe_root = unsafe_existing.join("jackin-hook-evidence");
        let before = fs::symlink_metadata(&unsafe_root)?;
        let error = persist(&unsafe_existing, b"must be rejected\n", &controls())
            .expect_err("unsafe existing evidence root must not be repaired");
        let after = fs::symlink_metadata(&unsafe_root)?;
        assert!(
            format!("{error:#}").contains("installation entry has an unexpected mode"),
            "unexpected unsafe-root rejection: {error:#}"
        );
        assert_eq!(
            (before.dev(), before.ino(), before.mode()),
            (after.dev(), after.ino(), after.mode())
        );
        return Ok(());
    }

    let temporary = tempfile::tempdir()?;
    let base = fs::canonicalize(temporary.path())?;
    let fresh = base.join("fresh.git");
    let unsafe_existing = base.join("unsafe.git");
    fs::create_dir(&fresh)?;
    fs::create_dir(&unsafe_existing)?;
    let unsafe_root = unsafe_existing.join("jackin-hook-evidence");
    fs::create_dir(&unsafe_root)?;
    fs::set_permissions(&unsafe_root, fs::Permissions::from_mode(0o755))?;

    let status = Command::new(std::env::current_exe()?)
        .args(["--exact", EVIDENCE_UMASK_TEST, "--nocapture"])
        .env(EVIDENCE_UMASK_CHILD, "1")
        .env("JACKIN_EVIDENCE_FRESH_GIT_DIR", &fresh)
        .env("JACKIN_EVIDENCE_UNSAFE_GIT_DIR", &unsafe_existing)
        .status()?;
    anyhow::ensure!(status.success(), "evidence umask child failed: {status}");
    let after = fs::symlink_metadata(&unsafe_root)?;
    assert_eq!(after.permissions().mode() & 0o777, 0o755);
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
    let patch_file = fs::File::from(super::open(
        &patch,
        super::OFlag::O_RDONLY | super::OFlag::O_NOFOLLOW | super::OFlag::O_CLOEXEC,
        super::Mode::empty(),
    )?);
    cleanup.record_patch(super::fstat(&patch_file)?);
    super::fchmod(&patch_file, super::Mode::from_bits_truncate(0o000))?;
    assert_eq!(patch_file.metadata()?.permissions().mode() & 0o777, 0);
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
