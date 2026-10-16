// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::CommandRunner;

fn shell() -> jackin_docker::shell_runner::ShellRunner {
    jackin_docker::shell_runner::ShellRunner::default()
}

async fn git(runner: &mut impl CommandRunner, args: &[&str]) {
    runner
        .run("git", args, None, &jackin_core::RunOptions::default())
        .await
        .unwrap();
}

async fn make_host_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("host");
    std::fs::create_dir_all(&repo).unwrap();
    let mut runner = shell();
    git(&mut runner, &["init", "-q", &repo.to_string_lossy()]).await;
    git(
        &mut runner,
        &[
            "-C",
            &repo.to_string_lossy(),
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "seed",
        ],
    )
    .await;
    std::fs::write(repo.join("file.txt"), "hello\n").unwrap();
    git(
        &mut runner,
        &[
            "-C",
            &repo.to_string_lossy(),
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "add",
            "-A",
        ],
    )
    .await;
    git(
        &mut runner,
        &[
            "-C",
            &repo.to_string_lossy(),
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "content",
        ],
    )
    .await;
    repo
}

#[tokio::test]
async fn local_clone_shares_objects_then_dissociates() {
    let dir = tempfile::TempDir::new().unwrap();
    let host = make_host_repo(dir.path()).await;
    let clone_path = dir.path().join("clone");
    let mut runner = shell();
    git(
        &mut runner,
        &[
            "clone",
            "--local",
            &host.to_string_lossy(),
            &clone_path.to_string_lossy(),
        ],
    )
    .await;

    // Precondition: the raw `--local` clone is NOT dissociated on at least
    // one axis (shared inodes or an alternates pointer).
    let raw_verdict = verify_no_hardlinked_objects(&host, &clone_path);
    let alternates = clone_path
        .join(".git")
        .join("objects")
        .join("info")
        .join("alternates");
    assert!(
        raw_verdict.is_err() || alternates.exists(),
        "expected a raw --local clone to share objects or record alternates",
    );

    dissociate_clone(&mut runner, &clone_path).await.unwrap();
    assert!(!alternates.exists(), "alternates pointer must be dropped");
    verify_no_hardlinked_objects(&host, &clone_path).unwrap();

    // The dissociated clone is still a functional repository at the same commit.
    let host_head: String = runner
        .capture(
            "git",
            &["-C", &host.to_string_lossy(), "rev-parse", "HEAD"],
            None,
        )
        .await
        .unwrap()
        .trim()
        .to_owned();
    let clone_head: String = runner
        .capture(
            "git",
            &["-C", &clone_path.to_string_lossy(), "rev-parse", "HEAD"],
            None,
        )
        .await
        .unwrap()
        .trim()
        .to_owned();
    assert_eq!(host_head, clone_head);
    // Host objects are untouched: the host head still verifies.
    runner
        .capture(
            "git",
            &["-C", &host.to_string_lossy(), "fsck", "--no-dangling"],
            None,
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn worktree_shares_git_and_must_not_be_used_for_workflows() {
    let dir = tempfile::TempDir::new().unwrap();
    let host = make_host_repo(dir.path()).await;
    let wt = dir.path().join("wt");
    let mut runner = shell();
    git(
        &mut runner,
        &[
            "-C",
            &host.to_string_lossy(),
            "worktree",
            "add",
            &wt.to_string_lossy(),
        ],
    )
    .await;

    // A worktree carries a `.git` *file* pointing back into the host
    // repository: shared writable Git metadata, forbidden for workflows.
    let git_entry = std::fs::symlink_metadata(wt.join(".git")).unwrap();
    assert!(
        git_entry.file_type().is_file(),
        "worktree .git must be a pointer file into the shared host repository",
    );
    let pointer = std::fs::read_to_string(wt.join(".git")).unwrap();
    assert!(
        pointer.starts_with("gitdir: "),
        "unexpected worktree pointer: {pointer:?}",
    );

    // The clone layout is disjoint from the worktree layout.
    let state_dir = dir.path().join("state");
    let worktree_path = crate::materialize::worktree_path_for(&state_dir, "/repo", "worker-1");
    let clone_path = crate::materialize::clone_path_for(&state_dir, "/repo", "worker-1");
    assert_ne!(worktree_path, clone_path);
}
