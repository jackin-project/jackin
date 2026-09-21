// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

fn canary_tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let outside = temp.path().join("outside");
    std::fs::create_dir_all(outside.join("dir")).unwrap();
    std::fs::write(outside.join("file.txt"), "canary-file").unwrap();
    std::fs::write(outside.join("dir").join("nested.txt"), "canary-nested").unwrap();
    let victim = temp.path().join("victim");
    (temp, outside, victim)
}

fn canaries_intact(outside: &Path) -> bool {
    std::fs::read_to_string(outside.join("file.txt"))
        .is_ok_and(|contents| contents == "canary-file")
        && std::fs::read_to_string(outside.join("dir").join("nested.txt"))
            .is_ok_and(|contents| contents == "canary-nested")
}

#[test]
fn removes_nested_tree_and_nothing_else() {
    let temp = tempfile::tempdir().unwrap();
    let victim = temp.path().join("victim");
    std::fs::create_dir_all(victim.join("a").join("b")).unwrap();
    std::fs::write(victim.join("a").join("b").join("c.txt"), "c").unwrap();
    std::fs::write(victim.join("d.txt"), "d").unwrap();
    let sibling = temp.path().join("sibling.txt");
    std::fs::write(&sibling, "sibling").unwrap();

    safe_remove_dir_all(&victim).unwrap();

    assert!(!victim.exists());
    assert_eq!(std::fs::read_to_string(&sibling).unwrap(), "sibling");
}

#[test]
fn missing_path_is_noop() {
    let temp = tempfile::tempdir().unwrap();
    safe_remove_dir_all(&temp.path().join("absent")).unwrap();
    safe_remove_dir_contained(temp.path(), &temp.path().join("absent")).unwrap();
}

#[test]
fn top_level_symlink_is_refused_and_target_survives() {
    let (_temp, outside, victim) = canary_tree();
    symlink(&outside, &victim).unwrap();

    let error = safe_remove_dir_all(&victim).unwrap_err();
    assert!(error.to_string().contains("symlink"), "{error}");

    assert!(canaries_intact(&outside));
    assert!(std::fs::symlink_metadata(&victim).is_ok_and(|meta| { meta.file_type().is_symlink() }));
}

#[test]
fn nested_symlinks_are_unlinked_not_followed() {
    let (_temp, outside, victim) = canary_tree();
    std::fs::create_dir_all(victim.join("sub")).unwrap();
    std::fs::write(victim.join("sub").join("inner.txt"), "inner").unwrap();
    symlink(outside.join("dir"), victim.join("evil-dir")).unwrap();
    symlink(outside.join("file.txt"), victim.join("evil-file")).unwrap();
    symlink(outside.join("does-not-exist"), victim.join("dangling")).unwrap();

    safe_remove_dir_all(&victim).unwrap();

    assert!(!victim.exists());
    assert!(canaries_intact(&outside));
}

#[test]
fn regular_file_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file.txt");
    std::fs::write(&file, "keep").unwrap();
    let error = safe_remove_dir_all(&file).unwrap_err();
    assert!(error.to_string().contains("not a directory"), "{error}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
}

#[test]
fn contained_removes_inside_root() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    let target = root.join("git").join("worktree").join("wt");
    std::fs::create_dir_all(target.join("sub")).unwrap();
    std::fs::write(target.join("sub").join("f.txt"), "f").unwrap();

    safe_remove_dir_contained(&root, &target).unwrap();

    assert!(!target.exists());
    assert!(root.join("git").join("worktree").exists());
}

#[test]
fn contained_rejects_sibling_escape_and_dotdot() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(&root).unwrap();
    let sibling = temp.path().join("sibling");
    std::fs::create_dir_all(&sibling).unwrap();
    std::fs::write(sibling.join("canary.txt"), "canary").unwrap();

    let error = safe_remove_dir_contained(&root, &sibling).unwrap_err();
    assert!(error.to_string().contains("escapes"), "{error}");
    assert_eq!(
        std::fs::read_to_string(sibling.join("canary.txt")).unwrap(),
        "canary"
    );

    let dotdot = root.join("sub").join("..").join("..").join("sibling");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    let error = safe_remove_dir_contained(&root, &dotdot).unwrap_err();
    assert!(
        error.to_string().contains("escapes") || error.to_string().contains("symlink"),
        "{error}"
    );
    assert_eq!(
        std::fs::read_to_string(sibling.join("canary.txt")).unwrap(),
        "canary"
    );
}

#[test]
fn contained_rejects_root_itself() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("state");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    let error = safe_remove_dir_contained(&root, &root).unwrap_err();
    assert!(error.to_string().contains("root itself"), "{error}");
    assert!(root.join("sub").exists());
}

#[test]
fn contained_rejects_symlinked_suffix() {
    let (temp, outside, _) = canary_tree();
    let root = temp.path().join("state");
    std::fs::create_dir_all(&root).unwrap();
    symlink(&outside, root.join("mid")).unwrap();
    let target = root.join("mid").join("dir");
    assert!(target.is_dir());

    let error = safe_remove_dir_contained(&root, &target).unwrap_err();
    assert!(
        error.to_string().contains("escapes") || error.to_string().contains("symlink"),
        "{error}"
    );
    assert!(canaries_intact(&outside));
}
