// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    env,
    ffi::OsString,
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

use crate::precommit_snapshot::{
    git::{LiveRepository, OwnedSnapshot},
    process::capture,
    toolchain::QualifiedTools,
};

const CHILD_MODE: &str = "JACKIN_ALTERNATE_OBJECTS_CHILD";
const CHILD_ROOT: &str = "JACKIN_ALTERNATE_OBJECTS_ROOT";
const CHILD_TEST: &str = "precommit_snapshot::git::tests::alternates::final_gate_reads_head_and_blob_from_a_quoted_relative_alternate";
const GATE_OUTPUT: &str = "JACKIN_ALTERNATE_OBJECTS_GATE_OUTPUT";
const GATE_EXPECTED: &str = "JACKIN_ALTERNATE_OBJECTS_GATE_EXPECTED";
const FILE_CONTENT: &[u8] = b"only in the alternate object store\n";

#[test]
fn final_gate_reads_head_and_blob_from_a_quoted_relative_alternate() {
    if env::var_os(CHILD_MODE).is_some() {
        let root = PathBuf::from(env::var_os(CHILD_ROOT).expect("fixture root"));
        run_alternate_only_gate(&root);
        return;
    }

    let fixture = tempfile::tempdir().expect("fixture directory");
    let root = fs::canonicalize(fixture.path()).expect("physical fixture root");
    let alternate_repository = root.join("alternate:\n object store");
    let live_repository = root.join("live repository");
    fs::create_dir(&alternate_repository).expect("alternate repository path");
    fs::create_dir(&live_repository).expect("live repository path");
    prepare_alternate_repository(&alternate_repository);
    create_alternate_only_live_repository(&alternate_repository, &live_repository);
    fs::create_dir(root.join("tmp")).expect("private snapshot temporary parent");

    let alternate_entry = "\"../alternate:\\n object store/.git/objects\"";
    let status = Command::new(env::current_exe().expect("test executable"))
        .args(["--exact", CHILD_TEST, "--nocapture"])
        .current_dir(&live_repository)
        .env_clear()
        .env("PATH", env::var_os("PATH").expect("test PATH"))
        .env("HOME", &root)
        .env("TMPDIR", root.join("tmp"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", alternate_entry)
        .env(CHILD_MODE, "1")
        .env(CHILD_ROOT, &live_repository)
        .status()
        .expect("run isolated alternate-object test process");
    assert!(status.success(), "alternate-object child failed: {status}");
}

fn prepare_alternate_repository(repository: &Path) {
    git_clean(repository, &["init", "--quiet"]);
    fs::write(repository.join("alternate-only.txt"), FILE_CONTENT)
        .expect("alternate-only source file");
    git_clean(repository, &["add", "alternate-only.txt"]);
    git_clean(repository, &["commit", "--quiet", "-m", "alternate only"]);
}

fn create_alternate_only_live_repository(alternate: &Path, live: &Path) {
    let oid = String::from_utf8(git_clean(alternate, &["rev-parse", "HEAD"]))
        .expect("fixture object id is UTF-8");
    let oid = oid.trim_end_matches('\n').to_owned();
    git_clean(live, &["init", "--quiet"]);
    let refs = live.join(".git/refs/heads");
    fs::write(live.join(".git/HEAD"), b"ref: refs/heads/alternate-only\n")
        .expect("write live HEAD reference");
    fs::write(refs.join("alternate-only"), format!("{oid}\n"))
        .expect("write live branch reference");
    fs::copy(alternate.join(".git/index"), live.join(".git/index"))
        .expect("copy index that refers only to alternate objects");
    fs::write(live.join("alternate-only.txt"), b"unstaged live worktree\n")
        .expect("write distinct live worktree content");
}

fn run_alternate_only_gate(root: &Path) {
    let live = LiveRepository::capture_at(root).expect("capture alternate-only repository");
    assert_eq!(
        git_inherited(root, &["cat-file", "-t", "HEAD"]).as_slice(),
        b"commit\n",
        "live Git resolves HEAD through the inherited alternate selector"
    );
    assert_eq!(
        git_inherited(root, &["show", "HEAD:alternate-only.txt"]).as_slice(),
        FILE_CONTENT,
        "live Git resolves the committed blob through the inherited alternate selector"
    );
    let index_before = fs::read(&live.index).expect("live index bytes");
    let live_objects = store_contents(&live.objects);
    let alternate_store_before = store_contents(&alternate_object_directory(root));
    let worktree_before = fs::read(root.join("alternate-only.txt")).expect("live worktree bytes");

    let mut snapshot = OwnedSnapshot::create(&live).expect("create private snapshot");
    let tree = snapshot
        .capture_staged_tree()
        .expect("capture alternate-only tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize alternate-only tree");
    assert_eq!(
        fs::read(snapshot.root.join("alternate-only.txt")).expect("materialized blob"),
        FILE_CONTENT
    );

    let expected = snapshot.scratch_root.join("alternate-expected");
    let output = snapshot.scratch_root.join("alternate-output");
    fs::write(&expected, FILE_CONTENT).expect("write gate oracle");
    let gate = snapshot.scratch_root.join("alternate-gate");
    fs::write(
        &gate,
        b"#!/bin/sh\nset -eu\ntest \"${GIT_ALTERNATE_OBJECT_DIRECTORIES+x}\" != x\ntest \"$(git cat-file -t HEAD)\" = commit\ngit show HEAD:alternate-only.txt > \"$JACKIN_ALTERNATE_OBJECTS_GATE_OUTPUT\"\ncmp \"$JACKIN_ALTERNATE_OBJECTS_GATE_OUTPUT\" \"$JACKIN_ALTERNATE_OBJECTS_GATE_EXPECTED\"\n",
    )
    .expect("write final-gate alternate lookup");
    fs::set_permissions(&gate, fs::Permissions::from_mode(0o700))
        .expect("make final-gate script executable");
    let tools = QualifiedTools {
        gate,
        environment: BTreeMap::from([
            (
                OsString::from("PATH"),
                env::var_os("PATH").expect("test PATH"),
            ),
            (
                OsString::from("HOME"),
                env::var_os("HOME").expect("test HOME"),
            ),
            (OsString::from("GIT_CONFIG_NOSYSTEM"), OsString::from("1")),
            (
                OsString::from("GIT_CONFIG_GLOBAL"),
                OsString::from("/dev/null"),
            ),
            (OsString::from(GATE_OUTPUT), output.as_os_str().to_owned()),
            (OsString::from(GATE_EXPECTED), expected.into_os_string()),
        ]),
        path_aliases: None,
    };
    let controls = snapshot.controls.clone();
    let completion = snapshot
        .run_gate(&tools, &[], &controls)
        .expect("run final gate without inherited alternate selector");
    assert!(
        completion.success,
        "alternate-only objects must remain readable"
    );
    assert_eq!(
        fs::read(output).expect("gate-read alternate blob"),
        FILE_CONTENT
    );
    OwnedSnapshot::verify_live_index(&live, &controls).expect("live index remains unchanged");
    assert_eq!(
        fs::read(&live.index).expect("live index remains"),
        index_before
    );
    assert_eq!(
        fs::read(root.join("alternate-only.txt")).expect("live worktree remains"),
        worktree_before
    );
    assert_eq!(store_contents(&live.objects), live_objects);
    assert_eq!(
        store_contents(&alternate_object_directory(root)),
        alternate_store_before
    );
}

fn alternate_object_directory(live_root: &Path) -> PathBuf {
    live_root
        .parent()
        .expect("live fixture parent")
        .join("alternate:\n object store/.git/objects")
}

fn git_clean(root: &Path, arguments: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(root)
        .env("PATH", env::var_os("PATH").expect("test PATH"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_SHALLOW_FILE",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        command.env_remove(key);
    }
    let output = capture(&mut command).expect("fixture Git starts");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn git_inherited(root: &Path, arguments: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    let output = capture(&mut command).expect("live Git starts");
    assert!(
        output.status.success(),
        "live git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn store_contents(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut contents = BTreeMap::new();
    collect_store_contents(root, root, &mut contents);
    contents
}

fn collect_store_contents(
    root: &Path,
    directory: &Path,
    contents: &mut BTreeMap<PathBuf, Vec<u8>>,
) {
    for entry in crate::fs_util::read_dir_sorted(directory).expect("read object store directory") {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).expect("object store entry metadata");
        let relative = path.strip_prefix(root).expect("entry within object store");
        if metadata.file_type().is_symlink() {
            contents.insert(
                relative.to_path_buf(),
                fs::read_link(&path)
                    .expect("object store symlink target")
                    .as_os_str()
                    .as_bytes()
                    .to_vec(),
            );
        } else if metadata.is_dir() {
            contents.insert(relative.to_path_buf(), Vec::new());
            collect_store_contents(root, &path, contents);
        } else if metadata.is_file() {
            contents.insert(
                relative.to_path_buf(),
                fs::read(&path).expect("object store bytes"),
            );
        }
    }
}
