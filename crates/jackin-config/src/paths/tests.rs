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
