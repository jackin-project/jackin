// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, File};
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use nix::errno::Errno;
use nix::fcntl::{AtFlags, renameat};
use nix::sys::stat::{FchmodatFlags, Mode, fchmodat, fstatat, mkdirat};

use super::super::*;
use super::support::{alias_child_name, overlong_socket_path, short_private_root};

const UMASK_CHILD_ROOT: &str = "JACKIN_TEST_SOCKET_ALIAS_CHILD_ROOT";
const UMASK_CHILD_CASE: &str = "JACKIN_TEST_SOCKET_ALIAS_CHILD_CASE";
const UMASK_TEST_TIMEOUT: Duration = Duration::from_secs(20);
const PRIVATE_CHILD_SUCCESS_TEST: &str =
    "socket_alias::tests::case_02::newly_created_child_is_private_under_restrictive_umasks";
const PRIVATE_CHILD_FAILURE_TEST: &str =
    "socket_alias::tests::case_02::normalization_failure_leaves_new_child_unchanged";
const PRIVATE_CHILD_REPLACEMENT_TEST: &str = "socket_alias::tests::case_02::normalizer_replacement_is_rejected_by_post_observation_identity_check";

#[test]
fn default_shared_root_and_explicit_private_root_pass_descriptor_validation() {
    let default = open_trusted_socket_alias_root(default_socket_alias_root())
        .expect("platform shared alias root is trusted");
    validate_trusted_socket_alias_root(&default, default_socket_alias_root())
        .expect("platform shared alias root mode");

    let private = short_private_root();
    let full = overlong_socket_path();
    let alias = expected_alias(private.path(), &full);
    if alias.as_os_str().as_encoded_bytes().len() < UNIX_SOCKET_PATH_LIMIT {
        let alias = short_socket_alias_with_root(&full, Some(private.path()))
            .expect("private alias provisioning")
            .expect("long path needs alias");
        assert_eq!(alias, expected_alias(private.path(), &full));
    } else {
        let _error = short_socket_alias_with_root(&full, Some(private.path())).unwrap_err();
    }
}

#[test]
fn canonical_private_root_spelling_is_resolved_before_descriptor_walk() {
    let private = short_private_root();
    let link_parent = tempfile::tempdir_in(default_socket_alias_root()).expect("link parent");
    let link = link_parent.path().join("l");
    symlink(private.path(), &link).expect("create intentional root spelling");
    let full = overlong_socket_path();
    if expected_alias(private.path(), &full)
        .as_os_str()
        .as_encoded_bytes()
        .len()
        < UNIX_SOCKET_PATH_LIMIT
    {
        let alias = short_socket_alias_with_root(&full, Some(&link))
            .expect("canonical root alias provisioning")
            .expect("long path needs alias");
        assert_eq!(alias, expected_alias(private.path(), &full));
    } else {
        let _error = short_socket_alias_with_root(&full, Some(&link)).unwrap_err();
    }
}

#[test]
fn writable_nonsticky_ancestor_is_rejected_without_changing_root() {
    let parent = tempfile::tempdir_in(default_socket_alias_root()).expect("parent");
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o770))
        .expect("make ancestor writable for the test");
    let root = parent.path().join("private");
    fs::create_dir(&root).expect("create private root");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("secure private root");
    let before = fs::symlink_metadata(&root).expect("root metadata");
    let _error = short_socket_alias_with_root(&overlong_socket_path(), Some(&root)).unwrap_err();
    let after = fs::symlink_metadata(&root).expect("root metadata after rejection");
    assert_eq!(before.mode(), after.mode());
    assert_eq!(before.uid(), after.uid());
}

#[test]
fn insecure_root_and_non_directory_fail_without_changing_existing_metadata() {
    let private = short_private_root();
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o755))
        .expect("make root insecure for the test");
    let before = fs::symlink_metadata(private.path()).expect("root metadata");
    let _error =
        short_socket_alias_with_root(&overlong_socket_path(), Some(private.path())).unwrap_err();
    let after = fs::symlink_metadata(private.path()).expect("root metadata after rejection");
    assert_eq!(before.mode(), after.mode());
    assert_eq!(before.uid(), after.uid());

    let temp = tempfile::tempdir().expect("tempdir");
    let file = temp.path().join("file");
    fs::write(&file, b"fixture").expect("create non-directory root");
    let _error = short_socket_alias_with_root(&overlong_socket_path(), Some(&file)).unwrap_err();
    assert_eq!(fs::read(&file).expect("non-directory remains"), b"fixture");
}

#[test]
fn insecure_existing_child_and_symlink_child_fail_without_repair_or_removal() {
    let private = short_private_root();
    let parent = open_trusted_socket_alias_root(private.path()).expect("trusted parent descriptor");
    let child = private.path().join(alias_child_name());
    for mode in [0o000, 0o500, 0o700, 0o755] {
        fs::create_dir(&child).expect("create existing child");
        fs::set_permissions(&child, fs::Permissions::from_mode(mode))
            .expect("set existing child mode");
        let before = fs::symlink_metadata(&child).expect("existing child metadata");
        let result = private_socket_alias_child(&parent, &alias_child_name());
        if mode == 0o700 {
            let directory = result.expect("secure existing child remains usable");
            assert_eq!(
                directory.metadata().expect("opened metadata").ino(),
                before.ino()
            );
            drop(directory);
        } else {
            let _error = result.expect_err("insecure existing child is rejected");
        }
        let after = fs::symlink_metadata(&child).expect("child remains unchanged");
        assert_eq!(after.file_type().is_dir(), before.file_type().is_dir());
        assert_eq!(after.uid(), before.uid());
        assert_eq!(after.ino(), before.ino());
        assert_eq!(after.mode() & 0o7777, mode);
        fs::remove_dir(&child).expect("remove existing child fixture");
    }

    let target = private.path().join("target");
    fs::create_dir(&target).expect("create symlink target");
    let target_before = fs::symlink_metadata(&target).expect("target metadata");
    symlink(&target, &child).expect("create symlink child");
    let link_before = fs::symlink_metadata(&child).expect("symlink metadata");
    let _error = private_socket_alias_child(&parent, &alias_child_name())
        .expect_err("symlink child is rejected without following it");
    let link_after = fs::symlink_metadata(&child).expect("symlink remains unchanged");
    assert!(link_after.file_type().is_symlink());
    assert_eq!(link_after.ino(), link_before.ino());
    assert_eq!(link_after.uid(), link_before.uid());
    let target_after = fs::symlink_metadata(&target).expect("target remains unchanged");
    assert_eq!(target_after.ino(), target_before.ino());
    assert_eq!(target_after.mode() & 0o7777, target_before.mode() & 0o7777);

    fs::remove_file(&child).expect("remove symlink fixture");
    fs::write(&child, b"fixture").expect("create regular-file child");
    fs::set_permissions(&child, fs::Permissions::from_mode(0o600))
        .expect("set regular-file child mode");
    let file_before = fs::symlink_metadata(&child).expect("regular-file metadata");
    let _error = private_socket_alias_child(&parent, &alias_child_name())
        .expect_err("regular-file child is rejected");
    let file_after = fs::symlink_metadata(&child).expect("regular file remains unchanged");
    assert!(file_after.file_type().is_file());
    assert_eq!(file_after.ino(), file_before.ino());
    assert_eq!(file_after.uid(), file_before.uid());
    assert_eq!(file_after.mode() & 0o7777, 0o600);
    assert_eq!(fs::read(&child).expect("file contents remain"), b"fixture");
}

#[test]
fn overlong_custom_alias_path_fails_before_creating_a_child() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("r".repeat(32));
    fs::create_dir(&root).expect("create long private root");
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700))
        .expect("secure long private root");
    let child = root.join(alias_child_name());

    let _error = short_socket_alias_with_root(&overlong_socket_path(), Some(&root)).unwrap_err();
    assert!(
        !child.exists(),
        "overlong root must fail before child creation"
    );
}

#[test]
fn concurrent_alias_creation_agrees_on_one_private_child_inode() {
    let private = short_private_root();
    let full = Arc::new(overlong_socket_path());
    let root = Arc::new(private.path().to_path_buf());
    let barrier = Arc::new(Barrier::new(8));
    let mut threads = Vec::new();
    for _ in 0..8 {
        let full = Arc::clone(&full);
        let root = Arc::clone(&root);
        let barrier = Arc::clone(&barrier);
        threads.push(thread::spawn(move || {
            barrier.wait();
            let alias = short_socket_alias_with_root(&full, Some(&root))
                .expect("concurrent alias provisioning")
                .expect("long socket path needs an alias");
            let root_fd = open_trusted_socket_alias_root(&root).expect("trusted root");
            let child_fd = private_socket_alias_child(&root_fd, &alias_child_name())
                .expect("same secure child");
            let metadata = child_fd.metadata().expect("child metadata");
            (alias, metadata.dev(), metadata.ino())
        }));
    }
    let results = threads
        .into_iter()
        .map(|thread| thread.join().expect("alias creator thread"))
        .collect::<Vec<_>>();
    assert!(results.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn newly_created_child_is_private_under_restrictive_umasks() {
    if let Some(root) = std::env::var_os(UMASK_CHILD_ROOT) {
        assert_eq!(
            std::env::var(UMASK_CHILD_CASE).expect("child case"),
            "create"
        );
        assert_new_child_under_umask(Path::new(&root));
        return;
    }

    for mask in ["0700", "0777"] {
        let private = short_private_root();
        run_isolated_umask_case(mask, "create", private.path(), PRIVATE_CHILD_SUCCESS_TEST);
        let child = private.path().join(alias_child_name());
        let metadata = fs::symlink_metadata(&child).expect("created child metadata");
        assert!(metadata.is_dir());
        assert_eq!(metadata.uid(), geteuid().as_raw());
        assert_eq!(metadata.mode() & 0o7777, 0o700);
    }
}

#[test]
fn normalization_failure_leaves_new_child_unchanged() {
    if let Some(root) = std::env::var_os(UMASK_CHILD_ROOT) {
        assert_eq!(
            std::env::var(UMASK_CHILD_CASE).expect("child case"),
            "fail-normalization"
        );
        assert_normalization_failure(Path::new(&root));
        return;
    }

    let private = short_private_root();
    run_isolated_umask_case(
        "0777",
        "fail-normalization",
        private.path(),
        PRIVATE_CHILD_FAILURE_TEST,
    );
    let child = private.path().join(alias_child_name());
    let metadata = fs::symlink_metadata(&child).expect("failed child remains for inspection");
    assert!(metadata.is_dir());
    assert_eq!(metadata.uid(), geteuid().as_raw());
    assert_eq!(metadata.mode() & 0o7777, 0);
}

#[test]
fn normalizer_replacement_is_rejected_by_post_observation_identity_check() {
    if let Some(root) = std::env::var_os(UMASK_CHILD_ROOT) {
        assert_eq!(
            std::env::var(UMASK_CHILD_CASE).expect("child case"),
            "replace-child"
        );
        assert_child_replacement_is_rejected(Path::new(&root));
        return;
    }

    let private = short_private_root();
    run_isolated_umask_case(
        "0777",
        "replace-child",
        private.path(),
        PRIVATE_CHILD_REPLACEMENT_TEST,
    );

    let retained = private.path().join("retained-original");
    let replacement = private.path().join(alias_child_name());
    let retained_metadata = fs::symlink_metadata(&retained).expect("retained original inode");
    let replacement_metadata =
        fs::symlink_metadata(&replacement).expect("replacement child metadata");
    assert!(retained_metadata.is_dir());
    assert!(replacement_metadata.is_dir());
    assert_eq!(retained_metadata.uid(), geteuid().as_raw());
    assert_eq!(replacement_metadata.uid(), geteuid().as_raw());
    assert_eq!(retained_metadata.mode() & 0o7777, 0);
    assert_eq!(replacement_metadata.mode() & 0o7777, 0o700);
    assert_ne!(
        (retained_metadata.dev(), retained_metadata.ino()),
        (replacement_metadata.dev(), replacement_metadata.ino())
    );
    fs::remove_dir(&retained).expect("remove known empty retained test inode");
    fs::remove_dir(&replacement).expect("remove known empty replacement test child");
}

#[test]
fn created_child_identity_change_is_rejected() {
    ensure_socket_alias_child_identity("u", (3, 5), (3, 5))
        .expect("same native device/inode identity is accepted");
    assert!(ensure_socket_alias_child_identity("u", (3, 5), (4, 5)).is_err());
    assert!(ensure_socket_alias_child_identity("u", (3, 5), (3, 6)).is_err());
    ensure_socket_alias_child_identity(
        "u",
        (nix::libc::dev_t::MIN, nix::libc::ino_t::MAX),
        (nix::libc::dev_t::MIN, nix::libc::ino_t::MAX),
    )
    .expect("equal native minimum device and maximum inode bits are accepted");
    ensure_socket_alias_child_identity(
        "u",
        (nix::libc::dev_t::MAX, nix::libc::ino_t::MAX),
        (nix::libc::dev_t::MAX, nix::libc::ino_t::MAX),
    )
    .expect("equal native maximum device and inode bits are accepted");
    ensure_socket_alias_child_identity(
        "u",
        (nix::libc::dev_t::MIN, nix::libc::ino_t::MAX),
        (nix::libc::dev_t::MAX, nix::libc::ino_t::MAX),
    )
    .expect_err("different native device bits are rejected");
    ensure_socket_alias_child_identity(
        "u",
        (nix::libc::dev_t::MIN, nix::libc::ino_t::MIN),
        (nix::libc::dev_t::MIN, nix::libc::ino_t::MAX),
    )
    .expect_err("different native inode bits are rejected");
}

fn assert_new_child_under_umask(root: &Path) {
    let parent = open_trusted_socket_alias_root(root).expect("trusted prepared parent");
    assert_umask_masks_requested_directory_mode(root, &parent);

    let child_name = alias_child_name();
    let child_path = root.join(&child_name);
    let child = private_socket_alias_child(&parent, &child_name)
        .expect("new child is normalized and opened");
    let metadata = child.metadata().expect("opened child metadata");
    assert_eq!(metadata.uid(), geteuid().as_raw());
    assert_eq!(metadata.mode() & 0o7777, 0o700);

    let endpoint = child_path.join("t.sock");
    let listener = UnixListener::bind(&endpoint).expect("private child accepts a socket endpoint");
    fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))
        .expect("secure test socket endpoint");
    let stream = UnixStream::connect(&endpoint).expect("private socket endpoint is usable");
    let (_accepted, _address) = listener.accept().expect("accept test socket client");
    drop(stream);
    drop(listener);
    fs::remove_file(&endpoint).expect("remove test socket endpoint");

    let before_retry = fs::symlink_metadata(&child_path).expect("child before retry");
    let retry =
        private_socket_alias_child(&parent, &child_name).expect("secure existing child is reused");
    let after_retry = fs::symlink_metadata(&child_path).expect("child after retry");
    assert_eq!(
        retry.metadata().expect("retry metadata").ino(),
        metadata.ino()
    );
    assert_eq!(after_retry.dev(), before_retry.dev());
    assert_eq!(after_retry.ino(), before_retry.ino());
    assert_eq!(after_retry.uid(), before_retry.uid());
    assert_eq!(after_retry.mode() & 0o7777, 0o700);
}

fn assert_normalization_failure(root: &Path) {
    let parent = open_trusted_socket_alias_root(root).expect("trusted prepared parent");
    assert_umask_masks_requested_directory_mode(root, &parent);

    let child_name = alias_child_name();
    let child_path = root.join(&child_name);
    let mut calls = 0;
    let _error = private_socket_alias_child_with_normalizer(&parent, &child_name, |_, _| {
        calls += 1;
        Err(Errno::EPERM)
    })
    .expect_err("normalization failure is fail-closed");
    assert_eq!(calls, 1, "normalization was attempted exactly once");

    let metadata = fs::symlink_metadata(&child_path).expect("unrepaired child remains");
    assert!(metadata.is_dir());
    assert_eq!(metadata.uid(), geteuid().as_raw());
    assert_eq!(metadata.mode() & 0o7777, 0);
}

fn assert_child_replacement_is_rejected(root: &Path) {
    let parent = open_trusted_socket_alias_root(root).expect("trusted prepared parent");
    assert_umask_masks_requested_directory_mode(root, &parent);

    let child_name = alias_child_name();
    let retained_name = "retained-original";
    let mut retained_identity = None;
    let mut calls = 0;
    let error = private_socket_alias_child_with_normalizer(&parent, &child_name, |parent, name| {
        calls += 1;
        renameat(parent, name, parent, retained_name)?;
        let retained = fstatat(parent, retained_name, AtFlags::AT_SYMLINK_NOFOLLOW)?;
        retained_identity = Some((retained.st_dev, retained.st_ino));
        mkdirat(parent, name, Mode::from_bits_truncate(0o700))?;
        fchmodat(
            parent,
            name,
            Mode::from_bits_truncate(0o700),
            FchmodatFlags::NoFollowSymlink,
        )?;
        Ok(())
    })
    .expect_err("post-normalization observation rejects a replacement inode");
    assert!(
        error.message.contains("changed device/inode"),
        "identity mismatch must be the rejection cause: {}",
        error.message
    );
    assert_eq!(calls, 1, "replacement normalizer called exactly once");

    let retained = fstatat(&parent, retained_name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .expect("original inode remains retained");
    let replacement = fstatat(&parent, child_name.as_str(), AtFlags::AT_SYMLINK_NOFOLLOW)
        .expect("replacement remains for parent inspection");
    let retained_identity = retained_identity.expect("captured original identity");
    assert_eq!(retained_identity, (retained.st_dev, retained.st_ino));
    assert_ne!(retained_identity, (replacement.st_dev, replacement.st_ino));
    assert_eq!(retained.st_mode & 0o7777, 0);
    assert_eq!(replacement.st_mode & 0o7777, 0o700);
}

fn assert_umask_masks_requested_directory_mode(root: &Path, parent: &File) {
    let probe_name = "umask-probe";
    mkdirat(parent, probe_name, Mode::from_bits_truncate(0o700))
        .expect("create umask probe directory");
    let probe = fstatat(parent, probe_name, AtFlags::AT_SYMLINK_NOFOLLOW)
        .expect("inspect umask probe without following links");
    assert_eq!(probe.st_mode & 0o7777, 0);
    fs::remove_dir(root.join(probe_name)).expect("remove umask probe directory");
}

fn run_isolated_umask_case(mask: &str, case: &str, root: &Path, exact_test: &str) {
    let stdout_path = root.join("umask-child.stdout");
    let stderr_path = root.join("umask-child.stderr");
    let stdout = File::create(&stdout_path).expect("create child stdout log");
    let stderr = File::create(&stderr_path).expect("create child stderr log");
    let executable = std::env::current_exe().expect("test executable");
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            "umask \"$1\"; shift; exec \"$@\"",
            "jackin-socket-alias-umask-child",
        ])
        .arg(mask)
        .arg(executable)
        .args(["--exact", exact_test, "--nocapture"])
        .env(UMASK_CHILD_ROOT, root)
        .env(UMASK_CHILD_CASE, case)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .expect("start isolated test child");
    let deadline = Instant::now() + UMASK_TEST_TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().expect("read child status") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ignored = child.kill();
            let status = child.wait().expect("reap timed-out child");
            panic!(
                "isolated umask test exceeded {UMASK_TEST_TIMEOUT:?} with status {status}; stdout:\n{}\nstderr:\n{}",
                fs::read_to_string(&stdout_path).unwrap_or_default(),
                fs::read_to_string(&stderr_path).unwrap_or_default()
            );
        }
        thread::park_timeout(Duration::from_millis(10));
    };
    let stdout = fs::read_to_string(&stdout_path).expect("read child stdout log");
    let stderr = fs::read_to_string(&stderr_path).expect("read child stderr log");
    assert!(
        status.success(),
        "isolated umask test {exact_test} failed with {status}; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("running 1 test"),
        "child must execute exactly one harness test; stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn expected_alias(root: &Path, full: &Path) -> PathBuf {
    root.join(alias_child_name()).join(socket_alias_name(full))
}
