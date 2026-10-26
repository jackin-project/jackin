// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::super::*;
use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

const CHILD_REPOSITORY: &str = "JACKIN_TEMP_PARENT_OWNER_CHILD_REPOSITORY";
const CHILD_TMPDIR: &str = "JACKIN_TEMP_PARENT_OWNER_CHILD_TMPDIR";

#[test]
fn accepts_private_parent_and_root_sticky_tmp() {
    let private = tempfile::tempdir().expect("private fixture directory");
    let checked = canonical_temporary_parent(private.path()).expect("validate private parent");
    assert_eq!(
        checked,
        fs::canonicalize(private.path()).expect("canonical private parent")
    );

    let sticky_tmp = fs::canonicalize("/tmp").expect("canonical standard temporary directory");
    canonical_temporary_parent(&sticky_tmp).expect("validate root-owned sticky temporary parent");
}

#[test]
fn rejects_private_parent_beneath_foreign_writable_ancestor() {
    let home = fs::canonicalize(env::var_os("HOME").expect("HOME is set"))
        .expect("canonical home directory");
    let fixture = tempfile::Builder::new()
        .prefix("jackin-temp-parent-ancestor-")
        .tempdir_in(home)
        .expect("fixture root");
    let writable_ancestor = fixture.path().join("writable-ancestor");
    fs::create_dir(&writable_ancestor).expect("create writable ancestor");
    fs::set_permissions(&writable_ancestor, fs::Permissions::from_mode(0o777))
        .expect("make ancestor foreign writable");
    let private_parent = writable_ancestor.join("private-parent");
    fs::create_dir(&private_parent).expect("create private leaf");
    fs::set_permissions(&private_parent, fs::Permissions::from_mode(0o700))
        .expect("protect leaf directory");

    let error = canonical_temporary_parent(&private_parent)
        .expect_err("unsafe ancestor must invalidate private leaf");
    assert!(format!("{error:#}").contains("temporary parent"));
}

#[test]
fn rejects_unsafe_tmpdir_before_owner_snapshot_and_capture() {
    if let Some(repository) = env::var_os(CHILD_REPOSITORY) {
        run_unsafe_tmpdir_owner_child(Path::new(&repository));
        return;
    }

    let home = fs::canonicalize(env::var_os("HOME").expect("HOME is set"))
        .expect("canonical home directory");
    let repository = tempfile::Builder::new()
        .prefix("jackin-temp-parent-owner-")
        .tempdir_in(home)
        .expect("fixture repository");
    fixture_repository(repository.path());

    let unsafe_tmpdir = repository.path().join("unsafe-tmp");
    fs::create_dir(&unsafe_tmpdir).expect("create unsafe temporary parent");
    fs::set_permissions(&unsafe_tmpdir, fs::Permissions::from_mode(0o700))
        .expect("make temporary parent private during Git capture");
    let index_before = fs::read(repository.path().join(".git/index")).expect("index before child");
    let staged_before = fs::read(repository.path().join("tracked.txt")).expect("file before child");

    let status = Command::new(env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "precommit_snapshot::process::tests::temp_parent::rejects_unsafe_tmpdir_before_owner_snapshot_and_capture",
            "--nocapture",
        ])
        .env(CHILD_REPOSITORY, repository.path())
        .env(CHILD_TMPDIR, &unsafe_tmpdir)
        .env("TMPDIR", &unsafe_tmpdir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_CONFIG")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_CONFIG_COUNT")
        .status()
        .expect("run isolated native owner child");
    assert!(status.success(), "native owner child failed: {status}");
    assert_eq!(
        fs::read(repository.path().join(".git/index")).expect("index after child"),
        index_before
    );
    assert_eq!(
        fs::read(repository.path().join("tracked.txt")).expect("file after child"),
        staged_before
    );
    assert!(
        crate::fs_util::read_dir_sorted(&unsafe_tmpdir)
            .expect("unsafe temporary parent remains empty")
            .is_empty(),
        "owner must reject the parent before creating private scratch"
    );
}

fn run_unsafe_tmpdir_owner_child(repository: &Path) {
    let temporary = PathBuf::from(env::var_os(CHILD_TMPDIR).expect("child TMPDIR"));
    env::set_current_dir(repository).expect("set repository as owner cwd");
    let live = crate::precommit_snapshot::git::LiveRepository::capture_at(repository)
        .expect("capture live repository with safe temporary parent");
    let index_before = fs::read(repository.join(".git/index")).expect("child index before owner");
    let worktree_before =
        fs::read(repository.join("tracked.txt")).expect("child file before owner");

    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o777))
        .expect("make temporary parent foreign writable after live capture");
    let error = crate::precommit_snapshot::git::OwnedSnapshot::create(&live)
        .err()
        .expect("snapshot constructor must reject unsafe parent");
    assert!(format!("{error:#}").contains("temporary parent"));
    assert!(
        crate::fs_util::read_dir_sorted(&temporary)
            .expect("unsafe temporary parent remains empty after snapshot rejection")
            .is_empty(),
        "snapshot must reject the parent before creating private scratch"
    );

    let error = crate::precommit_snapshot::run_with_budgets(
        &[],
        crate::precommit_snapshot::OwnerBudgets::production(),
    )
    .expect_err("owner must reject unsafe temporary parent before gate execution");
    assert!(format!("{error:#}").contains("temporary parent"));

    let error = private_capture_dir().expect_err("process capture must reject unsafe parent");
    assert!(format!("{error:#}").contains("temporary parent"));
    assert_eq!(
        fs::read(repository.join(".git/index")).expect("child index after owner"),
        index_before
    );
    assert_eq!(
        fs::read(repository.join("tracked.txt")).expect("child file after owner"),
        worktree_before
    );
    assert!(
        crate::fs_util::read_dir_sorted(&temporary)
            .expect("unsafe temporary parent remains available")
            .is_empty(),
        "neither owner path may create a temporary child"
    );
}

fn fixture_repository(root: &Path) {
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .expect("initialize fixture repository");
    assert!(status.success(), "git init failed: {status}");
    fs::write(root.join("tracked.txt"), b"staged bytes\n").expect("write tracked file");
    let status = Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .expect("stage fixture file");
    assert!(status.success(), "git add failed: {status}");
    fs::write(root.join("tracked.txt"), b"unstaged bytes\n").expect("write worktree change");
}
