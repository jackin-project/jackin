// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::safe_remove::{safe_remove_dir_all, safe_remove_dir_contained};

#[test]
fn safe_remove_dir_all_clears_nested_tree() {
    let root = tempfile::tempdir().expect("test tempdir");
    let canonical = std::fs::canonicalize(root.path()).expect("canonical tempdir");
    let tree = canonical.join("tree");
    let nested = tree.join("nested").join("leaf.txt");
    std::fs::create_dir_all(nested.parent().expect("nested parent")).expect("create nested dirs");
    std::fs::write(&nested, "payload").expect("write nested file");
    safe_remove_dir_all(&tree).expect("remove nested tree");
    assert!(!tree.exists());
    assert!(root.path().exists());
}

#[test]
fn contained_removal_refuses_paths_outside_root() {
    let root = tempfile::tempdir().expect("test tempdir");
    let canonical = std::fs::canonicalize(root.path()).expect("canonical tempdir");
    let outside = canonical
        .parent()
        .expect("tempdir parent")
        .join(format!("jackin-test-outside-{}", std::process::id()));
    std::fs::create_dir_all(&outside).expect("create outside dir");
    let marker = outside.join("marker.txt");
    std::fs::write(&marker, "keep me").expect("write marker");
    assert!(
        safe_remove_dir_contained(root.path(), &outside).is_err(),
        "contained removal must refuse paths outside root"
    );
    assert!(marker.exists());
    std::fs::remove_dir_all(&outside).expect("clean outside dir");
}
