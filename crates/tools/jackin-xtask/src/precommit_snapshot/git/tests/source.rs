// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;
use flate2::{Compression, write::ZlibEncoder};
use std::{
    ffi::OsStr,
    io::Write,
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

fn git_executable() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").expect("test PATH"))
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .and_then(|path| fs::canonicalize(path).ok())
        .expect("resolve real Git executable")
}

fn shell_quote(path: &Path) -> String {
    shell_quote_text(&path.to_string_lossy())
}

fn shell_quote_text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
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

fn staged_blob(root: &Path, path: &str) -> Vec<u8> {
    let object_spec = format!(":{path}");
    private_git(root, &["show", &object_spec])
}

fn write_loose_blob(path: &Path, content: &[u8]) {
    let mut object = format!("blob {}\0", content.len()).into_bytes();
    object.extend_from_slice(content);
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&object)
        .expect("compress replacement blob");
    fs::write(path, encoder.finish().expect("finish replacement blob"))
        .expect("write replacement loose object");
}

fn rejects_corrupt_private_blob(object_format: Option<&str>) {
    const STAGED_CONTENT: &[u8] = b"original\0payload";
    const REPLACEMENT_CONTENT: &[u8] = b"tampered\0payload";
    assert_eq!(STAGED_CONTENT.len(), REPLACEMENT_CONTENT.len());

    let fixture = materialized_fixture(object_format, |root| {
        fs::write(root.join("payload.bin"), STAGED_CONTENT).expect("binary staged blob");
        fs::write(root.join("empty.bin"), b"").expect("empty staged blob");
    });
    fixture
        .snapshot
        .admit(&fixture.tree)
        .expect("admit the unchanged real-Git fixture");

    let expected_oid = String::from_utf8(private_git(
        fixture.live.path(),
        &["rev-parse", ":payload.bin"],
    ))
    .expect("staged blob object ID is ASCII");
    let expected_oid = expected_oid.trim_end_matches('\n');
    assert_eq!(
        expected_oid.len(),
        if object_format.is_some() { 64 } else { 40 }
    );
    assert_eq!(
        String::from_utf8(private_git(
            fixture.live.path(),
            &["hash-object", "payload.bin"],
        ))
        .expect("real Git blob hash is ASCII")
        .trim_end_matches('\n'),
        expected_oid,
        "the expected object ID is the real Git hash of the staged bytes"
    );
    let empty_oid = String::from_utf8(private_git(
        fixture.live.path(),
        &["rev-parse", ":empty.bin"],
    ))
    .expect("empty blob object ID is ASCII");
    assert_eq!(
        String::from_utf8(private_git(
            fixture.live.path(),
            &["hash-object", "empty.bin"],
        ))
        .expect("real Git empty blob hash is ASCII")
        .trim_end_matches('\n'),
        empty_oid.trim_end_matches('\n'),
        "the zero-length blob ID uses Git's exact object header"
    );

    let loose_object = fixture
        .snapshot
        .objects
        .join(&expected_oid[..2])
        .join(&expected_oid[2..]);
    let original_loose_object = match fs::read(&loose_object) {
        Ok(object) => Some(object),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("read private loose object before mutation: {error}"),
    };
    fs::create_dir_all(loose_object.parent().expect("loose object shard"))
        .expect("create private loose object shard");

    fs::write(
        fixture.snapshot.root.join("payload.bin"),
        REPLACEMENT_CONTENT,
    )
    .expect("align materialized checkout with replacement blob");
    write_loose_blob(&loose_object, REPLACEMENT_CONTENT);
    assert_eq!(
        private_git(&fixture.snapshot.root, &["cat-file", "blob", expected_oid],),
        REPLACEMENT_CONTENT,
        "real Git must read the replacement bytes from the private loose object"
    );
    assert_ne!(
        String::from_utf8(private_git(
            &fixture.snapshot.root,
            &["hash-object", "payload.bin"],
        ))
        .expect("replacement Git blob hash is ASCII")
        .trim_end_matches('\n'),
        expected_oid,
        "the replacement bytes must not authenticate to the staged object ID"
    );
    assert_eq!(
        fixture
            .snapshot
            .capture_staged_tree()
            .expect("staged tree remains fixed"),
        fixture.tree,
        "the private index and recorded source tree remain unchanged"
    );

    let error = fixture
        .snapshot
        .admit(&fixture.tree)
        .expect_err("bytes under a staged object ID must authenticate to that ID");
    assert!(
        format!("{error:#}").contains("staged Git blob object ID"),
        "the corrupted loose object must fail at OID authentication: {error:#}"
    );
    assert_live_unchanged(&fixture);

    fs::write(fixture.snapshot.root.join("payload.bin"), STAGED_CONTENT)
        .expect("restore materialized staged bytes");
    if let Some(original) = original_loose_object {
        fs::write(&loose_object, original).expect("restore original private loose object");
    } else {
        fs::remove_file(&loose_object).expect("remove injected private loose object");
    }
    fixture
        .snapshot
        .admit(&fixture.tree)
        .expect("admit the restored real-Git fixture");
    assert_live_unchanged(&fixture);
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
fn authenticates_sha1_loose_blob_contents_against_the_staged_object_id() {
    rejects_corrupt_private_blob(None);
}

#[test]
fn authenticates_sha256_loose_blob_contents_against_the_staged_object_id() {
    rejects_corrupt_private_blob(Some("sha256"));
}

#[test]
fn reads_all_staged_blobs_with_one_git_batch_process_per_measurement_pass() {
    let mut fixture = materialized_fixture(None, |root| {
        fs::write(root.join("first.txt"), b"first payload\n").expect("first blob");
        fs::write(root.join("second.txt"), b"second payload\n").expect("second blob");
        symlink("first.txt", root.join("link")).expect("symlink blob");
    });
    let wrapper = fixture.snapshot.scratch_root.join("counting-git");
    let log = fixture.snapshot.scratch_root.join("cat-file.log");
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = cat-file ]; then printf '%s\\n' \"$*\" >> {}; fi\nexec {} \"$@\"\n",
        shell_quote(&log),
        shell_quote(&git_executable())
    );
    fs::write(&wrapper, script).expect("write transparent Git wrapper");
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700))
        .expect("make transparent Git wrapper executable");
    fixture
        .snapshot
        .set_test_git_program(&wrapper)
        .expect("use transparent Git wrapper");

    fixture
        .snapshot
        .admit(&fixture.tree)
        .expect("admit all content from bounded cat-file batch runs");
    let calls = fs::read_to_string(log).expect("read Git batch call count");
    assert_eq!(
        calls.lines().collect::<Vec<_>>(),
        ["cat-file --batch", "cat-file --batch"],
        "two measurement passes each use one batch process, regardless of blob count"
    );
    assert_live_unchanged(&fixture);
}

#[test]
fn materializes_raw_staged_blobs_despite_git_checkout_attributes() {
    let fixture = materialized_fixture(None, |root| {
        git(root, &["config", "core.autocrlf", "false"]);
        fs::create_dir(root.join("nested")).expect("nested attribute directory");
        fs::write(
            root.join(".gitattributes"),
            b"text.txt text eol=crlf\nidentity.txt ident\nencoded.txt working-tree-encoding=UTF-16LE\nfiltered.txt filter=sentinel\n",
        )
        .expect("root checkout attributes");
        fs::write(
            root.join("nested/.gitattributes"),
            b"payload.txt text eol=crlf ident\n",
        )
        .expect("nested checkout attributes");
        fs::write(root.join("text.txt"), b"text line\r\n").expect("CRLF text source");
        fs::write(root.join("identity.txt"), b"$Id$\n").expect("ident source");
        let encoded = "encoded text\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        fs::write(root.join("encoded.txt"), encoded).expect("UTF-16LE source");
        fs::write(root.join("filtered.txt"), b"filter source\n").expect("filter source");
        fs::write(root.join("nested/payload.txt"), b"nested text\r\n$Id$\r\n")
            .expect("nested text source");
    });

    let marker = fixture.snapshot.scratch_root.join("filter-ran");
    let filter = fixture.snapshot.scratch_root.join("sentinel-filter");
    fs::write(
        &filter,
        format!(
            "#!/bin/sh\ntouch {}\nprintf 'filtered\\n'\n",
            shell_quote(&marker)
        ),
    )
    .expect("write sentinel smudge filter");
    fs::set_permissions(&filter, fs::Permissions::from_mode(0o700))
        .expect("make sentinel filter executable");
    let filter_command = shell_quote(&filter);
    private_git(
        &fixture.snapshot.root,
        &["config", "filter.sentinel.smudge", &filter_command],
    );
    private_git(
        &fixture.snapshot.root,
        &["config", "filter.sentinel.required", "true"],
    );

    let live = LiveRepository::capture_at(fixture.live.path()).expect("recapture live repository");
    fixture
        .snapshot
        .materialize(&fixture.tree, &live)
        .expect("rematerialize with a configured sentinel filter");
    for path in [
        "text.txt",
        "identity.txt",
        "encoded.txt",
        "filtered.txt",
        "nested/payload.txt",
    ] {
        assert_eq!(
            fs::read(fixture.snapshot.root.join(path)).expect("read materialized blob"),
            staged_blob(fixture.live.path(), path),
            "private snapshot must contain the raw staged blob for {path}"
        );
    }
    fixture
        .snapshot
        .admit(&fixture.tree)
        .expect("raw staged bytes satisfy source admission");
    assert!(!marker.exists(), "the checkout filter must not run");
    assert_live_unchanged(&fixture);
}

#[test]
fn rejects_malformed_git_batch_framing_from_the_selected_git_process() {
    let mut fixture = materialized_fixture(None, |_| {});
    let wrapper = fixture.snapshot.scratch_root.join("mutating-git");
    let real_git = shell_quote(&git_executable());
    let baseline = format!("#!/bin/sh\nexec {real_git} \"$@\"\n");
    fs::write(&wrapper, baseline).expect("write transparent Git wrapper");
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700))
        .expect("make transparent Git wrapper executable");
    fixture
        .snapshot
        .set_test_git_program(&wrapper)
        .expect("select the real-Git wrapper");
    fixture
        .snapshot
        .admit(&fixture.tree)
        .expect("unmodified Git batch output is accepted");

    let oid = String::from_utf8(private_git(
        fixture.live.path(),
        &["rev-parse", ":baseline.txt"],
    ))
    .expect("baseline blob ID is ASCII");
    let oid = oid.trim_end_matches('\n');
    for (mutation, diagnostic) in [
        ("wrong-oid", "identity or length"),
        ("wrong-type", "identity or length"),
        ("wrong-size", "identity or length"),
        ("missing-object", "identity or length"),
        ("truncated-body", "expected staged Git blob"),
        ("wrong-terminator", "missing its terminator"),
        ("missing-terminator", "response terminator"),
        ("trailing-record", "stdout output exceeded"),
    ] {
        let response = match mutation {
            "wrong-oid" => format!("{} blob 9\nbaseline\n\n", "0".repeat(oid.len())),
            "wrong-type" => format!("{oid} tree 9\nbaseline\n\n"),
            "wrong-size" => format!("{oid} blob 8\nbaseline\n\n"),
            "missing-object" => format!("{oid} missing\n"),
            "truncated-body" => format!("{oid} blob 9\nbase"),
            "wrong-terminator" => format!("{oid} blob 9\nbaseline\nX"),
            "missing-terminator" => format!("{oid} blob 9\nbaseline\n"),
            "trailing-record" => String::new(),
            _ => unreachable!(),
        };
        let cat_file = if mutation == "trailing-record" {
            format!("{real_git} \"$@\" || exit\nprintf 'trailing\\n'\nexit 0")
        } else {
            format!(
                "cat >/dev/null\nprintf '%s' {}\nexit 0",
                shell_quote_text(&response)
            )
        };
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = cat-file ] && [ \"$2\" = --batch ]; then\n{cat_file}\nfi\nexec {real_git} \"$@\"\n"
        );
        fs::write(&wrapper, script).expect("write selected-Git batch mutation wrapper");
        let error = fixture
            .snapshot
            .admit(&fixture.tree)
            .expect_err("malformed cat-file batch framing must fail closed");
        assert!(
            format!("{error:#}").contains(diagnostic),
            "the {mutation} batch response should fail at {diagnostic}: {error:#}"
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
