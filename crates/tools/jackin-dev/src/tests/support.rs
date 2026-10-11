// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn test_repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .expect("crates/tools/jackin-dev should live three levels below repo root")
        .to_owned()
}

pub(super) fn git_repo_with_commit() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    run_git(temp.path(), ["init", "-b", "main"]);
    run_git(
        temp.path(),
        ["config", "user.email", "test@example.invalid"],
    );
    run_git(temp.path(), ["config", "user.name", "Test User"]);
    fs::write(temp.path().join("tracked.txt"), "base\n").unwrap();
    run_git(temp.path(), ["add", "tracked.txt"]);
    run_git(temp.path(), ["commit", "-m", "base"]);
    temp
}

pub(super) fn run_git<I, S>(dir: &Path, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "git command failed with {status}");
}
