// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[cfg(target_os = "linux")]
#[test]
fn captures_non_utf8_repository_and_linked_worktree_paths_without_live_mutation() {
    assert_captures_special_paths(
        OsString::from_vec(b"repository-\xff\n".to_vec()),
        OsString::from_vec(b"linked-worktree-\xfe\n".to_vec()),
    );
}

#[test]
fn captures_trailing_newline_repository_and_linked_worktree_paths_without_live_mutation() {
    assert_captures_special_paths(
        OsString::from("repository-\n"),
        OsString::from("linked-worktree-\n"),
    );
}

fn assert_captures_special_paths(repository_name: OsString, linked_name: OsString) {
    let temporary = tempfile::tempdir().expect("fixture parent");
    let repository = temporary.path().join(repository_name);
    fs::create_dir(&repository).expect("create special-byte repository path");
    git(&repository, &["init", "--quiet", "-b", "main"]);
    git(&repository, &["config", "user.name", "Fixture"]);
    git(
        &repository,
        &["config", "user.email", "fixture@example.invalid"],
    );
    fs::write(repository.join("staged.txt"), "base\n").expect("base file");
    git(&repository, &["add", "staged.txt"]);
    git(&repository, &["commit", "--quiet", "-m", "base"]);

    fs::write(repository.join("staged.txt"), "repository staged\n").expect("stage content");
    git(&repository, &["add", "staged.txt"]);
    fs::write(repository.join("staged.txt"), "repository worktree\n").expect("unstaged content");
    let live = LiveRepository::capture_at(&repository).expect("capture raw-byte repo path");
    assert_eq!(
        live.root.as_os_str().as_bytes(),
        fs::canonicalize(&repository)
            .expect("canonical repository path")
            .as_os_str()
            .as_bytes()
    );
    assert_snapshot_preserves_live_state(
        &live,
        &repository,
        b"repository staged\n",
        b"repository worktree\n",
    );

    let linked = temporary.path().join(linked_name);
    let mut add = Command::new("git");
    add.args(["worktree", "add", "--quiet", "-b", "linked"])
        .arg(&linked)
        .current_dir(&repository);
    let output = capture(&mut add).expect("start linked worktree creation");
    assert!(
        output.status.success(),
        "add special-byte linked worktree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(linked.join("staged.txt"), "linked staged\n").expect("linked stage content");
    git(&linked, &["add", "staged.txt"]);
    fs::write(linked.join("staged.txt"), "linked worktree\n").expect("linked worktree content");
    let linked_live = LiveRepository::capture_at(&linked).expect("capture raw-byte linked path");
    assert_ne!(linked_live.worktree_git_dir, linked_live.common_git_dir);
    assert_eq!(
        linked_live.root.as_os_str().as_bytes(),
        fs::canonicalize(&linked)
            .expect("canonical linked-worktree path")
            .as_os_str()
            .as_bytes()
    );
    assert_snapshot_preserves_live_state(
        &linked_live,
        &linked,
        b"linked staged\n",
        b"linked worktree\n",
    );
}

fn assert_snapshot_preserves_live_state(
    live: &LiveRepository,
    root: &Path,
    staged: &[u8],
    worktree: &[u8],
) {
    let index_before = fs::read(&live.index).expect("live index bytes");
    let worktree_before = fs::read(root.join("staged.txt")).expect("live worktree bytes");
    assert_eq!(worktree_before, worktree);
    let snapshot = OwnedSnapshot::create(live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("staged tree");
    snapshot
        .materialize(&tree, live)
        .expect("materialize staged tree");
    snapshot.admit(&tree).expect("admit staged tree");
    assert_eq!(
        fs::read(snapshot.root.join("staged.txt")).expect("staged snapshot file"),
        staged
    );
    assert_eq!(
        fs::read(&live.index).expect("live index remains"),
        index_before
    );
    assert_eq!(
        fs::read(root.join("staged.txt")).expect("live worktree remains"),
        worktree_before
    );
    OwnedSnapshot::verify_live_index(live, &live.controls)
        .expect("live index custody remains valid");
}

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
    let stale_shared_index = directory.path().join(".git/sharedindex.inactive");
    fs::write(&stale_shared_index, b"inactive split-index generation")
        .expect("inactive shared-index generation");
    let stale_non_regular = directory.path().join(".git/sharedindex.not-a-file");
    fs::create_dir(&stale_non_regular).expect("non-regular shared-index entry");
    git(
        directory.path(),
        &["sparse-checkout", "init", "--cone", "--sparse-index"],
    );
    git(directory.path(), &["sparse-checkout", "set", "included"]);
    assert!(!directory.path().join("excluded/b.txt").exists());

    let live = LiveRepository::capture_at(directory.path()).expect("capture sparse repository");
    let shared_index = live
        .shared_index
        .as_ref()
        .expect("Git identifies the active split-index backing file");
    assert!(shared_index.is_file());
    let index_before = fs::read(&live.index).expect("sparse index bytes");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    fs::remove_file(&stale_shared_index).expect("expire inactive shared-index generation");
    fs::remove_dir(&stale_non_regular).expect("remove unrelated non-regular entry");
    OwnedSnapshot::verify_live_index(&live, &live.controls)
        .expect("inactive shared-index maintenance leaves the active index unchanged");
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

    fs::write(shared_index, b"changed split index backing file")
        .expect("mutate split index backing file");
    assert!(OwnedSnapshot::verify_live_index(&live, &live.controls).is_err());
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
    let error = snapshot
        .materialize(&tree, &live)
        .expect_err("gitlinks must be rejected before checkout");
    assert!(format!("{error:#}").contains("staged submodules"));
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
