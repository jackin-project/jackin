// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![cfg(unix)]

use super::*;

use std::os::unix::fs::symlink;

use tempfile::tempdir;

#[test]
fn private_file_write_does_not_follow_ancestor_replaced_after_validation() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("role/auth");
    let detached = temp.path().join("detached-auth");
    let decoy = temp.path().join("decoy");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::create_dir(&decoy).unwrap();
    let target = parent.join("auth.json");

    reject_auth_path(&target).unwrap();
    std::fs::rename(&parent, &detached).unwrap();
    symlink(&decoy, &parent).unwrap();

    assert!(write_private_file(&target, "private-value").is_err());
    assert!(!detached.join("auth.json").exists());
    assert!(!decoy.join("auth.json").exists());
}

#[test]
fn private_file_write_succeeds_for_an_unchanged_parent() {
    let temp = tempdir().unwrap();
    let parent = temp.path().join("role/auth");
    std::fs::create_dir_all(&parent).unwrap();
    let target = parent.join("auth.json");

    write_private_file(&target, "private-value").unwrap();

    assert_eq!(std::fs::read_to_string(target).unwrap(), "private-value");
}
