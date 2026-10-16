// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_core::CommandRunner;
use std::collections::HashMap;
use std::path::PathBuf;

fn shell() -> jackin_docker::shell_runner::ShellRunner {
    jackin_docker::shell_runner::ShellRunner::default()
}

async fn git(runner: &mut impl CommandRunner, args: &[&str]) {
    runner
        .run("git", args, None, &jackin_core::RunOptions::default())
        .await
        .unwrap();
}

struct Fixture {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    commit: String,
    remote_path: String,
    registry: AuthorizedRemotes,
}

async fn fixture() -> Fixture {
    let dir = tempfile::TempDir::new().unwrap();
    let repo = dir.path().join("src");
    let remote = dir.path().join("remote.git");
    std::fs::create_dir_all(&repo).unwrap();
    let mut runner = shell();
    git(&mut runner, &["init", "-q", &repo.to_string_lossy()]).await;
    git(
        &mut runner,
        &["init", "-q", "--bare", &remote.to_string_lossy()],
    )
    .await;
    std::fs::write(repo.join("a.txt"), "v1\n").unwrap();
    let repo_s = repo.to_string_lossy().into_owned();
    git(
        &mut runner,
        &[
            "-C",
            &repo_s,
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
            &repo_s,
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "checkpoint",
        ],
    )
    .await;
    let commit: String = runner
        .capture("git", &["-C", &repo_s, "rev-parse", "HEAD"], None)
        .await
        .unwrap()
        .trim()
        .to_owned();
    let registry = AuthorizedRemotes::new(HashMap::from([(
        "test-remote".to_owned(),
        remote.to_string_lossy().into_owned(),
    )]));
    Fixture {
        _dir: dir,
        repo,
        commit,
        remote_path: remote.to_string_lossy().into_owned(),
        registry,
    }
}

fn remote_ref(run: &str, name: &str) -> String {
    format!("{}run-scoped/{name}", run_ref_prefix(run))
}

#[tokio::test]
async fn push_is_idempotent_on_same_commit() {
    let fx = fixture().await;
    let mut runner = shell();
    let remote_ref = remote_ref("run-1", "ckpt-0");
    let first = push_exact_ref(
        &mut runner,
        &fx.repo,
        &fx.commit,
        &fx.registry,
        "test-remote",
        &remote_ref,
        "run-1",
    )
    .await
    .unwrap();
    assert_eq!(
        first,
        PushOutcome::Pushed {
            remote_ref: remote_ref.clone(),
            commit: fx.commit.clone(),
        }
    );
    // Repeat the same push request: the ref still names the same commit.
    let second = push_exact_ref(
        &mut runner,
        &fx.repo,
        &fx.commit,
        &fx.registry,
        "test-remote",
        &remote_ref,
        "run-1",
    )
    .await
    .unwrap();
    assert_eq!(second, first);
    let listing: String = runner
        .capture("git", &["ls-remote", &fx.remote_path, &remote_ref], None)
        .await
        .unwrap();
    assert!(
        listing.starts_with(&fx.commit),
        "unexpected listing: {listing:?}"
    );
}

#[tokio::test]
async fn push_conflicts_on_different_commit() {
    let fx = fixture().await;
    let mut runner = shell();
    // Move the remote ref to a different commit through a second clone.
    let other = fx.repo.parent().unwrap().join("other");
    git(
        &mut runner,
        &[
            "clone",
            "-q",
            &fx.repo.to_string_lossy(),
            &other.to_string_lossy(),
        ],
    )
    .await;
    std::fs::write(other.join("b.txt"), "diverged\n").unwrap();
    let other_s = other.to_string_lossy().into_owned();
    git(
        &mut runner,
        &[
            "-C",
            &other_s,
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
            &other_s,
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "diverged",
        ],
    )
    .await;
    let remote_ref = remote_ref("run-1", "ckpt-0");
    git(
        &mut runner,
        &[
            "-C",
            &other_s,
            "push",
            "-q",
            &fx.remote_path,
            &format!("HEAD:{remote_ref}"),
        ],
    )
    .await;

    let outcome = push_exact_ref(
        &mut runner,
        &fx.repo,
        &fx.commit,
        &fx.registry,
        "test-remote",
        &remote_ref,
        "run-1",
    )
    .await
    .unwrap();
    match outcome {
        PushOutcome::Conflict {
            expected, found, ..
        } => {
            assert_eq!(expected, fx.commit);
            assert_ne!(found, fx.commit);
        }
        PushOutcome::Pushed { .. } => panic!("different commit must conflict, never force-update"),
    }
    // The remote ref still names the other commit: never force-updated.
    let listing: String = runner
        .capture("git", &["ls-remote", &fx.remote_path, &remote_ref], None)
        .await
        .unwrap();
    assert!(!listing.starts_with(&fx.commit));
}

#[tokio::test]
async fn push_rejects_outside_prefix_and_unknown_remote() {
    let fx = fixture().await;
    let mut runner = shell();
    let err = push_exact_ref(
        &mut runner,
        &fx.repo,
        &fx.commit,
        &fx.registry,
        "test-remote",
        "refs/heads/main",
        "run-1",
    )
    .await
    .expect_err("ref outside the run prefix must be rejected");
    assert!(format!("{err:#}").contains("run-scoped prefix"));

    let err = push_exact_ref(
        &mut runner,
        &fx.repo,
        &fx.commit,
        &fx.registry,
        "evil-remote",
        &remote_ref("run-1", "ckpt-0"),
        "run-1",
    )
    .await
    .expect_err("unknown remote must be rejected");
    assert!(format!("{err:#}").contains("not in the authorized backup registry"));

    let err = push_exact_ref(
        &mut runner,
        &fx.repo,
        "not-a-sha",
        &fx.registry,
        "test-remote",
        &remote_ref("run-1", "ckpt-0"),
        "run-1",
    )
    .await
    .expect_err("invalid commit must be rejected");
    assert!(format!("{err:#}").contains("invalid commit"));
}

#[tokio::test]
async fn failed_backup_preserves_local_data() {
    let fx = fixture().await;
    let mut runner = shell();
    let registry = AuthorizedRemotes::new(HashMap::from([(
        "dead-remote".to_owned(),
        "/nonexistent/path/remote.git".to_owned(),
    )]));
    let err = push_exact_ref(
        &mut runner,
        &fx.repo,
        &fx.commit,
        &registry,
        "dead-remote",
        &remote_ref("run-1", "ckpt-0"),
        "run-1",
    )
    .await
    .expect_err("push to a dead remote must fail");
    assert!(format!("{err:#}").contains("local checkpoint is preserved"));
    // Local data is untouched.
    assert_eq!(
        std::fs::read_to_string(fx.repo.join("a.txt")).unwrap(),
        "v1\n"
    );
    let head: String = runner
        .capture(
            "git",
            &["-C", &fx.repo.to_string_lossy(), "rev-parse", "HEAD"],
            None,
        )
        .await
        .unwrap()
        .trim()
        .to_owned();
    assert_eq!(head, fx.commit);
}
