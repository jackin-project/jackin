// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::os::unix::fs::symlink;
use std::path::PathBuf;

fn tempdir() -> tempfile::TempDir {
    // macOS TMPDIR commonly traverses /var -> /private/var. Fixtures use
    // canonical trusted roots; the resolver intentionally rejects aliases.
    tempfile::tempdir_in(std::fs::canonicalize(std::env::temp_dir()).unwrap()).unwrap()
}

fn canary_tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempdir();
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
    let temp = tempdir();
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
    let temp = tempdir();
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
    let temp = tempdir();
    let file = temp.path().join("file.txt");
    std::fs::write(&file, "keep").unwrap();
    let error = safe_remove_dir_all(&file).unwrap_err();
    assert!(error.to_string().contains("not a directory"), "{error}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "keep");
}

#[test]
fn contained_removes_inside_root() {
    let temp = tempdir();
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
    let temp = tempdir();
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
    let temp = tempdir();
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
    // The intermediate `mid` symlink is refused during fd-pinned
    // traversal, which reports the OS errno (ENOTDIR on macOS, ELOOP
    // on Linux) rather than a reason string, so pin the stable
    // refusal prefix and kind instead of errno text.
    assert!(
        error.to_string().starts_with("refusing to remove"),
        "{error}"
    );
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(canaries_intact(&outside));
}

#[test]
fn missing_target_does_not_bypass_lexical_containment() {
    let temp = tempdir();
    let root = temp.path().join("state");
    std::fs::create_dir(&root).unwrap();
    for target in [
        temp.path().join("outside-missing"),
        root.join("missing").join("..").join("target"),
        root.join("missing").join(".").join("target"),
    ] {
        assert!(
            safe_remove_dir_contained(&root, &target).is_err(),
            "{}",
            target.display()
        );
    }
    assert!(safe_remove_dir_contained(&root, Path::new("missing-relative")).is_err());
}

#[test]
fn missing_containment_root_is_refused() {
    let temp = tempdir();
    let root = temp.path().join("missing-root");
    assert!(safe_remove_dir_contained(&root, &root.join("missing-target")).is_err());
}

#[test]
fn inside_root_symlink_parent_is_refused() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let victim = root.join("real").join("victim");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("canary"), "keep").unwrap();
    symlink(root.join("real"), root.join("alias")).unwrap();
    assert!(safe_remove_dir_contained(&root, &root.join("alias").join("victim")).is_err());
    assert_eq!(
        std::fs::read_to_string(victim.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn pin_survives_ancestor_replacement_without_touching_replacement() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let target = root.join("middle").join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("owned"), "owned").unwrap();
    let pin = pin_dir_contained(&root, &target).unwrap().unwrap();
    assert_eq!(pin.path(), target);
    std::fs::rename(root.join("middle"), root.join("moved")).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    pin.remove().unwrap();
    assert!(!root.join("moved").join("target").exists());
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn pin_refuses_replacement_of_exact_target() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("owned"), "owned").unwrap();
    let pin = pin_dir_contained(&root, &target).unwrap().unwrap();
    std::fs::rename(&target, root.join("moved")).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    assert!(pin.remove().is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("moved").join("owned")).unwrap(),
        "owned"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn pinned_metadata_reads_refuse_symlinks_directories_and_oversized_files() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let target = root.join("target");
    std::fs::create_dir_all(target.join("directory")).unwrap();
    std::fs::write(target.join("regular"), "metadata").unwrap();
    std::fs::write(target.join("oversized"), vec![b'x'; 65_537]).unwrap();
    symlink(target.join("regular"), target.join("link")).unwrap();
    let pin = pin_dir_contained(&root, &target).unwrap().unwrap();
    assert_eq!(
        pin.read_file("regular").unwrap().as_deref(),
        Some("metadata")
    );
    assert!(pin.read_file("absent").unwrap().is_none());
    for name in [
        "link",
        "directory",
        "oversized",
        "../regular",
        "",
        "/regular",
    ] {
        assert!(pin.read_file(name).is_err(), "{name}");
    }
    let _error = pin.open_child_dir("link").unwrap_err();
    assert!(pin.open_child_dir("directory").unwrap().is_some());
    assert!(pin.open_child_dir("absent").unwrap().is_none());
    let mut first = pin.entry_names().unwrap();
    let mut second = pin.entry_names().unwrap();
    first.sort();
    second.sort();
    assert_eq!(first, second);
    assert_eq!(first.len(), 4);
}

#[test]
fn lexical_absolute_pin_refuses_symlinked_ancestor() {
    let temp = tempdir();
    std::fs::create_dir_all(temp.path().join("real").join("target")).unwrap();
    symlink(temp.path().join("real"), temp.path().join("alias")).unwrap();
    let _error = pin_dir(&temp.path().join("alias").join("target")).unwrap_err();
}

#[test]
fn containment_root_symlink_is_refused() {
    let temp = tempdir();
    let actual_root = temp.path().join("actual-root");
    let root_alias = temp.path().join("root-alias");
    let victim = actual_root.join("victim");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("canary"), "keep").unwrap();
    symlink(&actual_root, &root_alias).unwrap();
    assert!(safe_remove_dir_contained(&root_alias, &root_alias.join("victim")).is_err());
    assert_eq!(
        std::fs::read_to_string(victim.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn admitted_removal_is_readonly_until_consumed() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let target = root.join("sockets");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("owned"), "owned").unwrap();
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    assert_eq!(
        std::fs::read_to_string(target.join("owned")).unwrap(),
        "owned"
    );
    admitted.remove().unwrap();
    assert!(!target.exists());
}

#[test]
fn admitted_absence_is_readonly_and_consumes_unchanged_absence() {
    let temp = tempdir();
    let root = temp.path().join("state");
    std::fs::create_dir(&root).unwrap();
    let target = root.join("sockets");
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    assert!(!target.exists());
    admitted.remove().unwrap();
    assert!(!target.exists());
}

#[test]
fn admitted_absence_refuses_a_directory_that_appeared() {
    let temp = tempdir();
    let root = temp.path().join("state");
    std::fs::create_dir(&root).unwrap();
    let target = root.join("sockets");
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    assert!(admitted.remove().is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn admitted_missing_ancestor_refuses_appeared_state() {
    let temp = tempdir();
    let root = temp.path().join("state");
    std::fs::create_dir(&root).unwrap();
    let target = root.join("missing-parent").join("sockets");
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    assert!(admitted.remove().is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn admitted_present_refuses_replaced_ancestor_and_preserves_both_trees() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let target = root.join("middle").join("sockets");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("owned"), "owned").unwrap();
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    std::fs::rename(root.join("middle"), root.join("moved")).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    assert!(admitted.remove().is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("moved").join("sockets").join("owned")).unwrap(),
        "owned"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn admitted_absence_refuses_replaced_root() {
    let temp = tempdir();
    let root = temp.path().join("state");
    std::fs::create_dir(&root).unwrap();
    let target = root.join("sockets");
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    std::fs::rename(&root, temp.path().join("old-state")).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    assert!(admitted.remove().is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}

#[test]
fn admitted_present_refuses_replaced_target() {
    let temp = tempdir();
    let root = temp.path().join("state");
    let target = root.join("sockets");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("owned"), "owned").unwrap();
    let admitted = OwnedRemoval::admit_contained(&root, &target).unwrap();
    std::fs::rename(&target, root.join("moved")).unwrap();
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("canary"), "keep").unwrap();
    assert!(admitted.remove().is_err());
    assert_eq!(
        std::fs::read_to_string(root.join("moved").join("owned")).unwrap(),
        "owned"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("canary")).unwrap(),
        "keep"
    );
}
