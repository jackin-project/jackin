// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::{
    ffi::OsStr,
    os::unix::ffi::{OsStrExt, OsStringExt},
    sync::atomic::Ordering,
};

struct MaterializedFixture {
    live: TempDir,
    snapshot: OwnedSnapshot,
    tree: OsString,
    live_index: PathBuf,
    live_index_bytes: Vec<u8>,
    live_entries: Vec<(Vec<u8>, u8, u32, Vec<u8>)>,
}

fn materialized_fixture(
    object_format: Option<&str>,
    populate: impl FnOnce(&Path),
) -> MaterializedFixture {
    let live = tempfile::tempdir().expect("fixture directory");
    let mut init = vec!["init", "--quiet"];
    if let Some(object_format) = object_format {
        init.push("--object-format");
        init.push(object_format);
    }
    git(live.path(), &init);
    fs::write(live.path().join("baseline.txt"), b"baseline\n").expect("initial file");
    git(live.path(), &["add", "baseline.txt"]);
    git(live.path(), &["commit", "--quiet", "-m", "initial"]);

    populate(live.path());
    git(live.path(), &["add", "--all"]);

    let repository = LiveRepository::capture_at(live.path()).expect("capture live repository");
    let live_index = repository.index.clone();
    let live_index_bytes = fs::read(&live_index).expect("read live index");
    let live_entries = capture_live_entries(live.path());
    let snapshot = OwnedSnapshot::create(&repository).expect("create private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &repository)
        .expect("materialize staged tree");
    MaterializedFixture {
        live,
        snapshot,
        tree,
        live_index,
        live_index_bytes,
        live_entries,
    }
}

fn private_git(root: &Path, arguments: &[&str]) -> Vec<u8> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .args(arguments)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
    let output = capture(&mut command).expect("run isolated Git command");
    assert!(
        output.status.success(),
        "private Git command {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn capture_live_entries(root: &Path) -> Vec<(Vec<u8>, u8, u32, Vec<u8>)> {
    fn visit(root: &Path, directory: &Path, entries: &mut Vec<(Vec<u8>, u8, u32, Vec<u8>)>) {
        let children = crate::fs_util::read_dir_sorted(directory).expect("enumerate live fixture");
        for child in children {
            if directory == root && child.file_name().as_bytes() == b".git" {
                continue;
            }
            let path = child.path();
            let relative = path
                .strip_prefix(root)
                .expect("live fixture path is under root")
                .as_os_str()
                .as_bytes()
                .to_vec();
            let metadata = fs::symlink_metadata(&path).expect("live fixture metadata");
            let file_type = metadata.file_type();
            let (tag, content) = if file_type.is_dir() {
                (b'd', Vec::new())
            } else if file_type.is_file() {
                (b'f', fs::read(&path).expect("live fixture bytes"))
            } else if file_type.is_symlink() {
                (
                    b'l',
                    fs::read_link(&path)
                        .expect("live fixture link target")
                        .as_os_str()
                        .as_bytes()
                        .to_vec(),
                )
            } else {
                panic!("fixture contains a special file");
            };
            entries.push((relative, tag, metadata.mode(), content));
            if file_type.is_dir() {
                visit(root, &path, entries);
            }
        }
    }

    let mut entries = Vec::new();
    visit(root, root, &mut entries);
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    entries
}

fn assert_live_unchanged(fixture: &MaterializedFixture) {
    assert_eq!(
        fs::read(&fixture.live_index).expect("live index remains readable"),
        fixture.live_index_bytes,
        "private source measurement must not change the live index"
    );
    assert_eq!(
        capture_live_entries(fixture.live.path()),
        fixture.live_entries,
        "private source measurement must not change the live checkout"
    );
}

#[test]
fn admits_sha1_and_sha256_trees_with_raw_paths_modes_and_internal_links() {
    for object_format in [None, Some("sha256")] {
        let fixture = materialized_fixture(object_format, |root| {
            fs::write(root.join("plain.txt"), b"payload\n").expect("plain file");
            let executable = root.join("run-me");
            fs::write(&executable, b"#!/bin/sh\nexit 0\n").expect("executable file");
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))
                .expect("executable mode");
            symlink("plain.txt", root.join("internal-link")).expect("internal symlink");
            fs::write(root.join("line\nbreak"), b"newline path\n").expect("newline path");
            fs::write(root.join("tab\tname"), b"tab path\n").expect("tab path");
            #[cfg(not(target_os = "macos"))]
            fs::write(
                root.join(OsString::from_vec(vec![
                    b'r', b'a', b'w', b'-', 0xff, b'-', b'p',
                ])),
                b"non-UTF-8 path\n",
            )
            .expect("non-UTF-8 path");
        });

        fixture
            .snapshot
            .admit(&fixture.tree)
            .expect("admit exact staged source tree");
        assert_ne!(
            fs::metadata(fixture.snapshot.root.join("run-me"))
                .expect("staged executable metadata")
                .permissions()
                .mode()
                & 0o111,
            0
        );
        assert_eq!(
            fs::read_link(fixture.snapshot.root.join("internal-link"))
                .expect("staged symlink target"),
            PathBuf::from("plain.txt")
        );
        assert_live_unchanged(&fixture);
    }
}

#[test]
fn synthetic_tree_record_preserves_non_utf8_and_control_bytes_in_paths() {
    let record = b"100644 blob 0000000000000000000000000000000000000000 3\traw\xff\tname\nline";
    assert_eq!(
        super::super::snapshot::tree_record_path_for_test(record)
            .expect("parse raw Git pathname bytes"),
        b"raw\xff\tname\nline"
    );
}

#[test]
fn rejects_a_valid_but_different_supplied_tree() {
    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("staged.txt"), b"staged\n").expect("staged source");
    });
    let mut command = Command::new("git");
    command
        .current_dir(fixture.live.path())
        .args(["rev-parse", "HEAD^{tree}"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    let output = capture(&mut command).expect("read a valid different tree object ID");
    assert!(output.status.success());
    let different_tree = OsString::from_vec(
        output
            .stdout
            .strip_suffix(b"\n")
            .expect("tree protocol newline")
            .to_vec(),
    );
    assert_ne!(different_tree, fixture.tree);
    assert!(
        fixture
            .snapshot
            .admit(&different_tree)
            .expect_err("a different tree cannot describe the materialized index")
            .to_string()
            .contains("differs from the materialized private index")
    );
    assert_live_unchanged(&fixture);
}

#[test]
fn pins_a_symbolic_tree_once_before_source_measurement() {
    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("staged.txt"), b"staged\n").expect("staged source");
    });
    let tree = std::str::from_utf8(fixture.tree.as_bytes()).expect("tree object ID is ASCII");
    let commit = private_git(
        &fixture.snapshot.root,
        &["commit-tree", tree, "-m", "fixture"],
    );
    let commit = String::from_utf8(commit)
        .expect("private commit object ID is UTF-8")
        .trim_end_matches('\n')
        .to_owned();
    let reference = "refs/jackin-test/staged-tree";
    private_git(&fixture.snapshot.root, &["update-ref", reference, &commit]);

    fixture
        .snapshot
        .admit_materialized_tree_with_hook(OsStr::new(reference), 1024 * 1024, || {
            private_git(&fixture.snapshot.root, &["update-ref", reference, "HEAD"]);
            Ok(())
        })
        .expect("measurement keeps using the single resolved tree after ref movement");
    assert_live_unchanged(&fixture);
}

#[test]
fn rejects_same_length_source_replacement_and_mode_change() {
    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("payload.txt"), b"AAAA\n").expect("staged source");
    });
    fs::write(fixture.snapshot.root.join("payload.txt"), b"BBBB\n")
        .expect("replace source with same-length bytes");
    let error = fixture
        .snapshot
        .admit(&fixture.tree)
        .expect_err("same-length replacement differs from the pinned Git blob");
    assert!(
        format!("{error:#}").contains("staged Git blob")
            || format!("{error:#}").contains("changed after directory enumeration"),
        "same-length source mutation should be rejected: {error:#}"
    );
    assert_live_unchanged(&fixture);

    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("payload.txt"), b"AAAA\n").expect("staged source");
    });
    fs::set_permissions(
        fixture.snapshot.root.join("payload.txt"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("change staged executable bit");
    assert!(
        format!(
            "{:#}",
            fixture
                .snapshot
                .admit(&fixture.tree)
                .expect_err("mode changes must differ from the staged tree")
        )
        .contains("mode differs from the staged tree")
    );
    assert_live_unchanged(&fixture);
}

#[test]
fn rejects_removed_truncated_and_grown_sources() {
    for replacement in [None, Some(b"x".as_slice()), Some(b"much longer".as_slice())] {
        let fixture = materialized_fixture(None, |root| {
            fs::write(root.join("payload.txt"), b"payload\n").expect("staged source");
        });
        let path = fixture.snapshot.root.join("payload.txt");
        if let Some(bytes) = replacement {
            fs::write(&path, bytes).expect("change materialized source length");
        } else {
            fs::remove_file(&path).expect("remove staged source");
        }
        assert!(
            fixture.snapshot.admit(&fixture.tree).is_err(),
            "removed, truncated, and grown sources must all be rejected"
        );
        assert_live_unchanged(&fixture);
    }
}

#[test]
fn rejects_ignored_untracked_target_entries() {
    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join(".gitignore"), b"target/\n").expect("ignore build output");
        fs::write(root.join("payload.txt"), b"staged\n").expect("staged source");
    });
    fs::create_dir(fixture.snapshot.root.join("target")).expect("untracked target directory");
    fs::write(fixture.snapshot.root.join("target/ignored.bin"), b"ignored")
        .expect("ignored untracked source");
    let error = fixture
        .snapshot
        .admit(&fixture.tree)
        .expect_err("ignored target contents remain outside the admitted tree");
    assert!(
        format!("{error:#}").contains("unexpected directory"),
        "unexpected target directory should fail tree membership: {error:#}"
    );
    assert_live_unchanged(&fixture);
}

#[test]
fn rejects_source_mutation_between_measurement_passes() {
    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("payload.txt"), b"before\n").expect("staged source");
    });
    let path = fixture.snapshot.root.join("payload.txt");
    let error = fixture
        .snapshot
        .admit_materialized_tree_with_hook(&fixture.tree, 1024 * 1024, || {
            fs::write(path, b"after!\n").expect("mutate source between passes");
            Ok(())
        })
        .expect_err("source changes between enumeration passes must fail");
    assert!(
        format!("{error:#}").contains("changed")
            || format!("{error:#}").contains("staged Git blob"),
        "mid-measurement mutation should be detected: {error:#}"
    );
    assert_live_unchanged(&fixture);
}

#[test]
fn rejects_symlink_substitution_and_mid_measurement_addition() {
    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("payload"), b"payload\n").expect("link source");
        fs::write(root.join("present"), b"present\n").expect("replacement target");
        symlink("payload", root.join("source-link")).expect("staged internal link");
    });
    fs::remove_file(fixture.snapshot.root.join("source-link")).expect("remove staged link");
    symlink("present", fixture.snapshot.root.join("source-link"))
        .expect("replace with a different internal target");
    assert!(fixture.snapshot.admit(&fixture.tree).is_err());
    assert_live_unchanged(&fixture);

    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("payload.txt"), b"staged\n").expect("staged source");
    });
    let added = fixture.snapshot.root.join("zz-newly-added.txt");
    let error = fixture
        .snapshot
        .admit_materialized_tree_with_hook(&fixture.tree, 1024 * 1024, || {
            fs::write(&added, b"new source\n").expect("add source between passes");
            Ok(())
        })
        .expect_err("new source membership between passes must fail");
    assert!(
        format!("{error:#}").contains("changed during measurement")
            || format!("{error:#}").contains("unexpected"),
        "new source membership should fail measurement: {error:#}"
    );
    assert_live_unchanged(&fixture);
}

#[test]
fn bounds_source_inventory_and_observes_cancellation_without_live_mutation() {
    let fixture = materialized_fixture(None, |root| {
        for index in 0..5 {
            fs::write(
                root.join(format!("long-source-filename-{index:02}.txt")),
                b"x\n",
            )
            .expect("write bounded inventory fixture");
        }
    });
    let error = fixture
        .snapshot
        .admit_materialized_tree_with_hook(&fixture.tree, 1024, || Ok(()))
        .expect_err("measured source inventory exceeds a small limit");
    assert!(
        format!("{error:#}").contains("inventory exceeds its byte limit"),
        "expected the materialized-source accounting limit, got {error:#}"
    );
    assert_live_unchanged(&fixture);

    let fixture = materialized_fixture(None, |root| {
        fs::write(root.join("payload.txt"), b"staged\n").expect("staged source");
    });
    let error = fixture
        .snapshot
        .admit_materialized_tree_with_hook(&fixture.tree, 1024 * 1024, || {
            fixture
                .snapshot
                .controls
                .cancellation()
                .store(true, Ordering::SeqCst);
            Ok(())
        })
        .expect_err("cancellation between passes stops measurement");
    assert!(
        crate::precommit_snapshot::process::is_clean_cancellation(&error),
        "cancellation should retain its typed classification: {error:#}"
    );
    assert_live_unchanged(&fixture);
}
