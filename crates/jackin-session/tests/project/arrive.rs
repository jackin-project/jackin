// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::RepoFixture;

use super::oracle::arrived;
use super::support::{Rig, TestResult, guard, ready};

async fn base() -> Result<RepoFixture, Box<dyn std::error::Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.file("b.txt", b"beta\n").await?;
    repo.file("run.sh", b"#!/bin/sh\n").await?;
    repo.chmod("run.sh", 0o755).await?;
    repo.ignore("ignored.log\n").await?;
    repo.commit().await?;
    Ok(repo)
}

async fn arrives(repo: RepoFixture, dirty: &[&str]) -> TestResult {
    let rig = Rig::new(repo).await?;
    ready(rig.launch(1).await?)?;
    let handle = rig.handle(1).await?;
    arrived(rig.repo.root().as_path(), &handle.root(), dirty)
}

#[tokio::test]
async fn a_normal_repository_arrives_with_its_files_refs_and_unstaged_changes() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.git(&["branch", "feat"]).await?;
        repo.git(&["branch", "master"]).await?;
        repo.git(&["tag", "v1"]).await?;
        repo.git(&["update-ref", "refs/remotes/origin/main", "HEAD"])
            .await?;
        repo.file("a.txt", b"alpha staged\n").await?;
        repo.git(&["add", "a.txt"]).await?;
        repo.file("b.txt", b"beta unstaged\n").await?;
        repo.file("n.txt", b"new and staged\n").await?;
        repo.git(&["add", "n.txt"]).await?;
        repo.file("u.txt", b"untracked\n").await?;
        repo.file("ignored.log", b"ignored\n").await?;
        arrives(repo, &[" M a.txt", " M b.txt", "?? n.txt", "?? u.txt"]).await
    })
    .await
}

#[tokio::test]
async fn symlinks_arrive_as_symlinks_for_a_file_a_directory_and_a_dangling_target() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.file("real/inner.txt", b"inner\n").await?;
        let root = repo.root().as_path().to_path_buf();
        for (name, target) in [
            ("link.txt", "a.txt"),
            ("dirlink", "real"),
            ("dangling", "nowhere"),
        ] {
            std::os::unix::fs::symlink(target, root.join(name))?;
        }
        repo.commit().await?;
        arrives(repo, &[]).await
    })
    .await
}

#[tokio::test]
async fn a_linked_worktree_root_arrives_as_a_normal_repository() -> TestResult {
    guard(async {
        let main = base().await?;
        let tree = main.linked_worktree("topic").await?;
        tree.file("a.txt", b"edited in the worktree\n").await?;
        tree.file("u.txt", b"untracked\n").await?;
        assert!(tree.root().as_path().join(".git").is_file());
        arrives(tree, &[" M a.txt", "?? u.txt"]).await
    })
    .await
}

#[tokio::test]
async fn a_linked_worktree_in_a_directory_whose_name_holds_a_line_feed_arrives() -> TestResult {
    guard(async {
        let main = base().await?;
        let tree = main.linked_worktree_named("topic", "line\nbreak").await?;
        tree.file("u.txt", b"untracked\n").await?;
        assert!(tree.root().as_path().join(".git").is_file());
        arrives(tree, &["?? u.txt"]).await
    })
    .await
}

#[tokio::test]
async fn a_submodule_checkout_root_arrives_with_its_detached_head() -> TestResult {
    guard(async {
        let source = base().await?;
        let checkout = source.submodule_checkout().await?;
        checkout.git(&["checkout", "-q", "--detach"]).await?;
        assert!(checkout.root().as_path().join(".git").is_file());
        checkout
            .git(&["symbolic-ref", "-q", "HEAD"])
            .await
            .unwrap_err();
        arrives(checkout, &[]).await
    })
    .await
}

#[tokio::test]
async fn a_sha256_repository_arrives_as_sha256() -> TestResult {
    guard(async {
        let repo = RepoFixture::new_sha256().await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.commit().await?;
        assert_eq!(
            repo.git(&["rev-parse", "--show-object-format"]).await?,
            "sha256\n"
        );
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        let format = super::oracle::git(
            &handle.root().join("project"),
            &["rev-parse", "--show-object-format"],
        )?;
        assert_eq!(format, "sha256\n");
        arrived(rig.repo.root().as_path(), &handle.root(), &[])
    })
    .await
}

#[tokio::test]
async fn a_shallow_clone_arrives_with_the_same_shallow_file() -> TestResult {
    guard(async {
        let source = base().await?;
        source.file("a.txt", b"second\n").await?;
        source.commit().await?;
        let shallow = source.clone_of(&["--depth", "1"]).await?;
        assert_eq!(
            shallow
                .git(&["rev-parse", "--is-shallow-repository"])
                .await?,
            "true\n"
        );
        let rig = Rig::new(shallow).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        arrived(rig.repo.root().as_path(), &handle.root(), &[])?;
        assert_eq!(
            std::fs::read(handle.root().join("project/.git/shallow"))?,
            std::fs::read(rig.repo.root().as_path().join(".git/shallow"))?
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unborn_branch_arrives_unborn_with_every_other_branch() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.git(&["branch", "feat"]).await?;
        repo.git(&["checkout", "-q", "--orphan", "fresh"]).await?;
        repo.git(&["rev-parse", "-q", "--verify", "HEAD"])
            .await
            .unwrap_err();
        arrives(
            repo,
            &["?? .gitignore", "?? a.txt", "?? b.txt", "?? run.sh"],
        )
        .await
    })
    .await
}

#[tokio::test]
async fn a_repository_with_files_and_no_refs_launches_with_an_unborn_head() -> TestResult {
    guard(async {
        let repo = RepoFixture::new().await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.file("sub/b.txt", b"beta\n").await?;
        arrives(repo, &["?? a.txt", "?? sub/b.txt"]).await
    })
    .await
}

#[tokio::test]
async fn a_detached_head_arrives_detached_at_the_same_commit_with_the_branches() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.file("a.txt", b"second\n").await?;
        repo.commit().await?;
        repo.git(&["branch", "feat"]).await?;
        repo.git(&["checkout", "-q", "--detach", "HEAD~1"]).await?;
        arrives(repo, &[]).await
    })
    .await
}

#[tokio::test]
async fn a_project_in_a_directory_whose_name_holds_a_line_feed_arrives() -> TestResult {
    guard(async {
        let repo = RepoFixture::new_named("line\nbreak").await?;
        repo.file("a.txt", b"alpha\n").await?;
        repo.commit().await?;
        repo.file("b.txt", b"beta\n").await?;
        arrives(repo, &["?? b.txt"]).await
    })
    .await
}

#[tokio::test]
async fn the_public_upload_entry_rebuilds_the_repository_in_an_existing_sandbox() -> TestResult {
    guard(async {
        let repo = base().await?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        rig.repo.file("a.txt", b"edited after the launch\n").await?;
        rig.repo.file("late.txt", b"late\n").await?;
        rig.world
            .session
            .upload_project(
                rig.project(),
                &handle.name(),
                &super::support::launch_id(2),
                &tokio_util::sync::CancellationToken::new(),
            )
            .await?;
        arrived(
            rig.repo.root().as_path(),
            &handle.root(),
            &[" M a.txt", "?? late.txt"],
        )?;
        let left = super::support::tree_of(rig.world.cache_dir().as_path())?;
        assert!(
            left.iter().all(|name| !name.contains(".bundle")),
            "{left:?}"
        );
        Ok(())
    })
    .await
}
