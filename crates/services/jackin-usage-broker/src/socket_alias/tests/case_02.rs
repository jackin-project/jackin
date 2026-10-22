// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;

use super::super::*;
use super::support::{alias_child_name, overlong_socket_path, short_private_root};

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
    let child = private.path().join(alias_child_name());
    fs::create_dir(&child).expect("create existing child");
    fs::set_permissions(&child, fs::Permissions::from_mode(0o755))
        .expect("make child insecure for the test");
    let _error =
        short_socket_alias_with_root(&overlong_socket_path(), Some(private.path())).unwrap_err();
    assert_eq!(
        fs::metadata(&child).expect("child remains").mode() & 0o7777,
        0o755
    );

    fs::remove_dir(&child).expect("remove test child");
    let target = private.path().join("target");
    fs::create_dir(&target).expect("create symlink target");
    symlink(&target, &child).expect("create symlink child");
    let _error =
        short_socket_alias_with_root(&overlong_socket_path(), Some(private.path())).unwrap_err();
    assert!(
        fs::symlink_metadata(&child)
            .expect("symlink remains")
            .file_type()
            .is_symlink()
    );
    assert!(target.is_dir());
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

fn expected_alias(root: &Path, full: &Path) -> PathBuf {
    root.join(alias_child_name()).join(socket_alias_name(full))
}
