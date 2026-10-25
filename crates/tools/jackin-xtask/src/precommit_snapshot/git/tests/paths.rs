// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn git_path_record_parser_preserves_non_utf8_and_trailing_newline_bytes() {
    assert_eq!(
        super::super::without_record_terminator(b"/repo/\xff\n\n").expect("one record terminator"),
        b"/repo/\xff\n"
    );
    super::super::without_record_terminator(b"/repo/\xff")
        .expect_err("missing record terminator must fail");
}

#[test]
fn git_relative_path_overrides_resolve_from_repository_root_and_snapshot_selected_split_index() {
    const CHILD_MARKER: &str = "JACKIN_RELATIVE_GIT_PATH_CHILD";
    const FIXTURE_ROOT: &str = "JACKIN_RELATIVE_GIT_PATH_FIXTURE";
    const TEST_NAME: &str = "precommit_snapshot::git::tests::paths::git_relative_path_overrides_resolve_from_repository_root_and_snapshot_selected_split_index";

    if std::env::var_os(CHILD_MARKER).is_some() {
        let root = std::env::var_os(FIXTURE_ROOT).expect("child fixture root");
        let root = PathBuf::from(root);
        assert_child_git_path_resolution(&root);
        return;
    }

    let fixture = tempfile::tempdir().expect("fixture directory");
    let root = fs::canonicalize(fixture.path()).expect("physical repository path");
    prepare_relative_path_fixture(&root);

    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", TEST_NAME, "--nocapture"])
        .current_dir(root.join("nested"))
        .env(CHILD_MARKER, "1")
        .env(FIXTURE_ROOT, &root)
        .env("GIT_INDEX_FILE", "alternate.index")
        .env("GIT_OBJECT_DIRECTORY", ".git/objects")
        .env("GIT_SHALLOW_FILE", "alternate-shallow")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_INDEX_VERSION")
        .status()
        .expect("run isolated Git path admission child");
    assert!(
        status.success(),
        "Git path admission child failed: {status}"
    );
}

#[test]
fn snapshot_resolves_relative_tmpdir_before_child_git_commands() {
    const CHILD_MARKER: &str = "JACKIN_RELATIVE_TMPDIR_SNAPSHOT_CHILD";
    const FIXTURE_ROOT: &str = "JACKIN_RELATIVE_TMPDIR_SNAPSHOT_FIXTURE";
    const TEST_NAME: &str = "precommit_snapshot::git::tests::paths::snapshot_resolves_relative_tmpdir_before_child_git_commands";

    if std::env::var_os(CHILD_MARKER).is_some() {
        let root = PathBuf::from(std::env::var_os(FIXTURE_ROOT).expect("child fixture root"));
        let invocation = root.join("nested");
        let live = LiveRepository::capture_at(&invocation).expect("capture fixture repository");
        let snapshot = OwnedSnapshot::create(&live).expect("create absolute private snapshot");
        let expected_temp_parent =
            fs::canonicalize(root.join("relative tmp")).expect("physical relative TMPDIR");

        assert!(snapshot.scratch_root.is_absolute());
        assert!(snapshot.root.is_absolute());
        assert!(snapshot.git_dir.is_absolute());
        assert_eq!(
            snapshot.scratch_root.parent(),
            Some(expected_temp_parent.as_path())
        );

        let tree = snapshot
            .capture_staged_tree()
            .expect("run Git from snapshot child cwd");
        snapshot
            .materialize(&tree, &live)
            .expect("materialize snapshot tree");

        std::env::set_current_dir(&root).expect("change parent process cwd");
        let mut command = Command::new("git");
        command
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&snapshot.root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_COMMON_DIR");
        let output = capture(&mut command).expect("run Git from absolute snapshot cwd");
        assert!(output.status.success(), "snapshot child Git failed");
        assert_eq!(
            fs::canonicalize(
                String::from_utf8(output.stdout)
                    .expect("UTF-8 snapshot root")
                    .trim()
            )
            .expect("physical child Git root"),
            snapshot.root
        );
        return;
    }

    let fixture = tempfile::tempdir().expect("fixture directory");
    let root = fs::canonicalize(fixture.path()).expect("physical repository path");
    prepare_relative_path_fixture(&root);
    fs::create_dir(root.join("relative tmp")).expect("relative TMPDIR parent");

    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .args(["--exact", TEST_NAME, "--nocapture"])
        .current_dir(root.join("nested"))
        .env(CHILD_MARKER, "1")
        .env(FIXTURE_ROOT, &root)
        .env("TMPDIR", "../relative tmp")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .status()
        .expect("run relative-TMPDIR snapshot child");
    assert!(
        status.success(),
        "relative-TMPDIR snapshot child failed: {status}"
    );
}

fn assert_child_git_path_resolution(root: &Path) {
    let invocation = root.join("nested");
    let controls = SetupControls::for_test(std::time::Duration::from_mins(10));
    let discovered_root =
        super::super::output_live_at(&invocation, &["rev-parse", "--show-toplevel"], &controls)
            .expect("Git discovers repository from nested invocation");
    assert_eq!(
        fs::canonicalize(discovered_root).expect("physical discovered repository root"),
        root
    );
    assert_eq!(
        super::super::output_live_at(&invocation, &["cat-file", "-t", "HEAD"], &controls)
            .expect("Git reads repository HEAD from nested invocation"),
        "commit"
    );

    let shallow_state = super::super::output_live_at(
        &invocation,
        &["rev-parse", "--is-shallow-repository"],
        &controls,
    )
    .expect("Git shallow state");
    assert_eq!(shallow_state, "true");
    let git_index = super::super::output_live_path_at(
        &invocation,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
        &controls,
    )
    .expect("Git-selected index path");
    let git_objects = super::super::output_live_path_at(
        &invocation,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
        ],
        &controls,
    )
    .expect("Git-selected object directory");
    let git_shared_index = super::super::output_live_optional_path_at(
        &invocation,
        &["rev-parse", "--path-format=absolute", "--shared-index-path"],
        &controls,
    )
    .expect("Git shared-index path query")
    .expect("Git selected shared-index backing");
    let git_directory = super::super::output_live_path_at(
        &invocation,
        &["rev-parse", "--absolute-git-dir"],
        &controls,
    )
    .expect("Git-selected worktree directory");

    // Git can refresh a split-index backing file's mtime while answering path
    // queries, so capture custody only after all Git oracle queries complete.
    let live = LiveRepository::capture_at(&invocation).expect("capture nested repository");
    assert_eq!(live.index, git_index);
    assert_eq!(live.index, root.join("alternate.index"));
    assert_eq!(live.objects, git_objects);
    assert_eq!(live.objects, root.join(".git/objects"));
    let expected_shallow = root.join("alternate-shallow");
    assert_eq!(live.shallow.as_deref(), Some(expected_shallow.as_path()));
    let shared_index = live
        .shared_index
        .as_ref()
        .expect("Git-selected index uses split-index backing");
    assert_eq!(shared_index, &git_shared_index);
    assert_eq!(shared_index.parent(), Some(git_directory.as_path()));

    let snapshot = OwnedSnapshot::create(&live).expect("create snapshot");
    let tree = snapshot
        .capture_staged_tree()
        .expect("capture Git-selected alternate index tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize selected alternate index");
    assert_eq!(
        fs::read(snapshot.root.join("staged.txt")).expect("selected staged file"),
        b"selected staged\n"
    );
}

fn prepare_relative_path_fixture(root: &Path) {
    git_without_path_overrides(root, &["init", "--quiet"], None);
    fs::write(root.join("staged.txt"), b"old\n").expect("initial file");
    git_without_path_overrides(root, &["add", "staged.txt"], None);
    git_without_path_overrides(root, &["commit", "--quiet", "-m", "initial"], None);
    fs::write(root.join("staged.txt"), b"staged\n").expect("initial staged bytes");
    git_without_path_overrides(root, &["add", "staged.txt"], None);
    fs::write(root.join("staged.txt"), b"unstaged\n").expect("initial unstaged bytes");
    fs::create_dir(root.join("nested")).expect("nested invocation directory");
    fs::create_dir_all(root.join("nested/.git/objects"))
        .expect("nested object-directory discovery decoy");
    let alternate_index = root.join("alternate.index");
    fs::copy(root.join(".git/index"), &alternate_index).expect("copy primary index");
    fs::write(root.join("staged.txt"), b"selected staged\n").expect("selected staged bytes");
    git_without_path_overrides(root, &["add", "staged.txt"], Some(&alternate_index));
    git_without_path_overrides(
        root,
        &["update-index", "--split-index"],
        Some(&alternate_index),
    );
    fs::copy(root.join(".git/index"), root.join("nested/alternate.index"))
        .expect("write distinct nested decoy index");
    fs::write(root.join("staged.txt"), b"unstaged after alternate\n")
        .expect("leave live worktree unstaged");
    let head = git_without_path_overrides(root, &["rev-parse", "HEAD"], None).stdout;
    let head = String::from_utf8(head)
        .expect("fixture HEAD is UTF-8")
        .trim()
        .to_owned();
    fs::write(root.join("alternate-shallow"), format!("{head}\n"))
        .expect("root-relative shallow override");
}

fn git_without_path_overrides(
    root: &Path,
    arguments: &[&str],
    index: Option<&Path>,
) -> std::process::Output {
    let mut command = Command::new("git");
    command
        .args(arguments)
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_SHALLOW_FILE")
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_INDEX_VERSION");
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = capture(&mut command).expect("Git starts with alternate index");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        arguments.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn collects_tracked_and_new_path_fixes_against_captured_tree() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize tree");
    fs::write(snapshot.root.join("staged.txt"), "fixed\n").expect("tracked fix");
    fs::write(snapshot.root.join("created file\nname"), "new\n").expect("new fix");
    let patch = snapshot
        .collect_fixes(&tree, &snapshot.controls)
        .expect("collect patch");
    assert!(patch.starts_with(b"diff --git "));
    assert!(String::from_utf8_lossy(&patch).contains("created file\\nname"));
    assert!(String::from_utf8_lossy(&patch).contains("fixed"));
    assert_eq!(
        fs::read(fixture.path().join("staged.txt")).expect("live file"),
        b"unstaged\n"
    );
}

#[test]
fn collects_new_files_whose_names_look_like_git_pathspec_magic() {
    let fixture = fixture();
    let live = LiveRepository::capture_at(fixture.path()).expect("capture fixture");
    let snapshot = OwnedSnapshot::create(&live).expect("private snapshot");
    let tree = snapshot.capture_staged_tree().expect("capture staged tree");
    snapshot
        .materialize(&tree, &live)
        .expect("materialize staged tree");
    let filename = ":(exclude)*";
    fs::write(
        snapshot.root.join(filename),
        "must be included in evidence\n",
    )
    .expect("create pathspec-shaped filename");
    let patch = snapshot
        .collect_fixes(&tree, &snapshot.controls)
        .expect("collect patch");
    let patch = String::from_utf8_lossy(&patch);
    assert!(patch.contains("must be included in evidence"));
    assert!(patch.contains("(exclude)*"));
}
