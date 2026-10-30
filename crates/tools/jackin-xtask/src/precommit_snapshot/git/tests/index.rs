// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn expands_split_sparse_index_in_private_tree() {
    let directory = tempfile::tempdir().expect("fixture directory");
    git(directory.path(), &["init", "--quiet"]);
    git(directory.path(), &["config", "user.name", "Fixture"]);
    git(
        directory.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::create_dir_all(directory.path().join("included")).expect("included directory");
    fs::create_dir_all(directory.path().join("excluded")).expect("excluded directory");
    fs::write(directory.path().join("included/a.txt"), "included\n").expect("included file");
    fs::write(directory.path().join("excluded/b.txt"), "excluded\n").expect("excluded file");
    git(directory.path(), &["add", "--all"]);
    git(directory.path(), &["commit", "--quiet", "-m", "fixture"]);
    git(directory.path(), &["update-index", "--split-index"]);
    git(
        directory.path(),
        &["sparse-checkout", "init", "--cone", "--sparse-index"],
    );
    git(directory.path(), &["sparse-checkout", "set", "included"]);
    assert!(!directory.path().join("excluded/b.txt").exists());

    let live = LiveRepository::capture_at(directory.path()).expect("capture sparse repository");
    let index_before = fs::read(&live.index).expect("sparse index bytes");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture sparse tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize full tree");
    snapshot.admit(&tree).expect("admit full tree");

    assert_eq!(
        fs::read(snapshot.root.join("excluded/b.txt")).unwrap(),
        b"excluded\n"
    );
    assert!(!snapshot.root.join(".git/info/sparse-checkout").exists());
    assert_eq!(
        fs::read(&live.index).expect("live sparse index"),
        index_before
    );
    assert!(!directory.path().join("excluded/b.txt").exists());

    let shared_index = live
        .index_inputs
        .iter()
        .find(|input| {
            input
                .path
                .file_name()
                .unwrap()
                .as_bytes()
                .starts_with(b"sharedindex.")
        })
        .expect("split index backing file captured");
    fs::write(&shared_index.path, b"changed split index backing file")
        .expect("mutate split index backing file");
    assert!(OwnedSnapshot::verify_live_index(&live).is_err());
}

#[test]
fn captures_unborn_staged_tree_without_creating_a_head() {
    let directory = tempfile::tempdir().expect("fixture directory");
    git(directory.path(), &["init", "--quiet"]);
    git(directory.path(), &["config", "user.name", "Fixture"]);
    git(
        directory.path(),
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::write(directory.path().join("first.txt"), "staged\n").expect("staged file");
    git(directory.path(), &["add", "first.txt"]);

    let live = LiveRepository::capture_at(directory.path()).expect("capture unborn repository");
    assert!(live.head.is_none());
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot
        .capture_staged_tree()
        .expect("capture unborn staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize unborn tree");
    snapshot.admit(&tree).expect("admit unborn tree");

    assert_eq!(
        fs::read(snapshot.root.join("first.txt")).unwrap(),
        b"staged\n"
    );
    let mut snapshot_git = Command::new("git");
    snapshot_git
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(snapshot.root.join("."));
    assert!(!capture(&mut snapshot_git).unwrap().status.success());
    let mut live_git = Command::new("git");
    live_git
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(directory.path());
    assert!(!capture(&mut live_git).unwrap().status.success());
}

#[test]
fn rejects_staged_escaping_symlinks_and_gitlinks_before_gate() {
    let fixture = fixture();
    let root = fixture.path();
    symlink("/outside", root.join("escape")).expect("absolute symlink");
    git(root, &["add", "escape"]);
    let live = LiveRepository::capture_at(root).expect("capture linked fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot
        .capture_staged_tree()
        .expect("capture linked staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize linked tree");
    assert!(snapshot.admit(&tree).is_err());

    git(root, &["reset", "--", "escape"]);
    fs::remove_file(root.join("escape")).expect("remove staged symlink");
    let mut command = Command::new("git");
    command.args(["rev-parse", "HEAD"]).current_dir(root);
    let head = capture(&mut command).expect("read fixture HEAD");
    assert!(head.status.success());
    let head = String::from_utf8(head.stdout).expect("HEAD object ID");
    let mut command = Command::new("git");
    command
        .args([
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},submodule", head.trim()),
        ])
        .current_dir(root);
    let output = capture(&mut command).expect("stage gitlink");
    assert!(output.status.success());
    let live = LiveRepository::capture_at(root).expect("capture gitlink fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private gitlink snapshot");
    let tree = snapshot
        .capture_staged_tree()
        .expect("capture gitlink tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize gitlink tree");
    assert!(snapshot.admit(&tree).is_err());
}

#[test]
fn linked_worktree_merge_metadata_is_copied_without_live_mutation() {
    let fixture = fixture();
    let root = fixture.path();
    let linked = root.join("linked-worktree");
    git(
        root,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    let live = LiveRepository::capture_at(&linked).expect("capture linked worktree");
    assert_ne!(live.worktree_git_dir, live.common_git_dir);
    let mut command = Command::new("git");
    command.args(["rev-parse", "HEAD"]).current_dir(&linked);
    let head = capture(&mut command).expect("read linked HEAD");
    assert!(head.status.success());
    let head = String::from_utf8(head.stdout).unwrap();
    let merge_head = live.worktree_git_dir.join("MERGE_HEAD");
    fs::write(&merge_head, &head).unwrap();

    let snapshot = OwnedSnapshot::create(&live).expect("private linked snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture linked tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize linked tree");
    snapshot.admit(&tree).expect("admit linked tree");
    assert_eq!(
        fs::read(snapshot.git_dir.join("MERGE_HEAD")).unwrap(),
        fs::read(&merge_head).unwrap()
    );
    let mut command = Command::new("git");
    command
        .args(["rev-parse", "MERGE_HEAD"])
        .current_dir(&snapshot.root);
    let merge = capture(&mut command).expect("read private merge head");
    assert!(merge.status.success());
    assert_eq!(merge.stdout, head.as_bytes());
}

#[test]
fn rejects_unmerged_index_without_changing_its_live_bytes() {
    let fixture = fixture();
    let root = fixture.path();
    let mut command = Command::new("git");
    command
        .args(["rev-parse", "HEAD:staged.txt"])
        .current_dir(root);
    let blob = capture(&mut command).expect("read base blob");
    assert!(blob.status.success());
    let blob = String::from_utf8(blob.stdout).unwrap();
    let input = format!(
        "100644 {} 1\tconflict.txt\n100644 {} 2\tconflict.txt\n",
        blob.trim(),
        blob.trim()
    );
    let mut child = Command::new("git")
        .args(["update-index", "--index-info"])
        .current_dir(root)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("start conflict index setup");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());

    let live = LiveRepository::capture_at(root).expect("capture conflicted index");
    let index_before = fs::read(&live.index).unwrap();
    let snapshot = OwnedSnapshot::create(&live).expect("private conflict snapshot");
    snapshot.capture_staged_tree().unwrap_err();
    assert_eq!(fs::read(&live.index).unwrap(), index_before);
}
