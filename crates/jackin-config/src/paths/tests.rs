// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use super::*;

#[test]
fn resolve_path_resolves_relative_to_cwd() {
    let cwd = std::env::current_dir().unwrap();
    let resolved = resolve_path("my-project");

    assert_eq!(resolved, cwd.join("my-project").display().to_string());
    assert!(resolved.starts_with('/'));
}

#[test]
fn resolve_path_leaves_absolute_unchanged() {
    assert_eq!(resolve_path("/workspace/project"), "/workspace/project");
}

#[test]
fn resolve_path_normalizes_dot_to_cwd() {
    let cwd = std::env::current_dir().unwrap();
    let resolved = resolve_path(".");

    assert_eq!(resolved, cwd.display().to_string());
}

#[test]
fn resolve_path_normalizes_parent_component() {
    let cwd = std::env::current_dir().unwrap();
    let resolved = resolve_path("../sibling");
    let expected = cwd.parent().unwrap().join("sibling");

    assert_eq!(resolved, expected.display().to_string());
    assert!(!resolved.contains(".."));
}

#[test]
fn resolve_path_normalizes_absolute_with_dotdot() {
    assert_eq!(resolve_path("/a/b/../c"), "/a/c");
}

#[test]
fn normalize_path_handles_multiple_parent_refs() {
    let path = Path::new("/a/b/c/../../d");
    assert_eq!(normalize_path(path), PathBuf::from("/a/d"));
}

#[test]
fn normalize_path_preserves_root_on_excessive_parents() {
    let path = Path::new("/a/../../../b");
    assert_eq!(normalize_path(path), PathBuf::from("/b"));
}

#[test]
fn canonical_path_identity_normalizes_missing_descendants() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    std::fs::create_dir(&real).unwrap();

    let spelled = real.join("created-later/../future");
    assert_eq!(
        canonical_path_identity(&spelled),
        real.canonicalize().unwrap().join("future")
    );
}

#[cfg(unix)]
#[test]
fn canonical_path_identity_resolves_symlinked_ancestors() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    let alias = temp.path().join("alias");
    std::fs::create_dir(&real).unwrap();
    symlink(&real, &alias).unwrap();

    assert_eq!(
        canonical_path_identity(&alias.join("created-later/../future")),
        real.canonicalize().unwrap().join("future")
    );
}
