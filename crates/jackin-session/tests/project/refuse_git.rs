// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use jackin_core::HostPath;
use jackin_session::UnsupportedProject;
use jackin_testkit::RepoFixture;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use super::support::{Rig, TestResult, assert_nothing_created, guard, ready, unsupported};

async fn base() -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Ok(repo)
}

async fn victim() -> Result<(RepoFixture, PathBuf), Box<dyn Error>> {
    let victim = RepoFixture::new().await?;
    victim.file("private.txt", b"TOP-SECRET-TOKEN\n").await?;
    victim.commit().await?;
    let git_dir = victim.root().as_path().join(".git");
    Ok((victim, git_dir))
}

async fn foreign(repo: RepoFixture, case: &str) -> TestResult {
    let root = repo.root().clone();
    let rig = Rig::new(repo).await?;
    match unsupported(rig.launch(1).await?)? {
        UnsupportedProject::ForeignGit(named) => assert_eq!(named, root, "{case}"),
        other => return Err(format!("{case}: expected ForeignGit, saw {other:?}").into()),
    }
    assert_nothing_created(&rig)
}

#[tokio::test]
async fn a_gitfile_whose_gitdir_names_the_project_but_borrows_a_foreign_commondir_is_refused()
-> TestResult {
    guard(async {
        let (_victim, victim_git) = victim().await?;
        let repo = base().await?;
        let root = repo.root().as_path();
        std::fs::rename(root.join(".git"), root.join(".jg"))?;
        std::fs::write(root.join(".git"), "gitdir: .jg\n")?;
        std::fs::write(root.join(".jg/gitdir"), "../.git\n")?;
        std::fs::write(
            root.join(".jg/commondir"),
            format!("{}\n", victim_git.display()),
        )?;
        foreign(
            repo,
            "gitfile with a back reference and a foreign commondir",
        )
        .await
    })
    .await
}

#[tokio::test]
async fn a_git_directory_that_carries_a_commondir_is_refused() -> TestResult {
    guard(async {
        let (_victim, victim_git) = victim().await?;
        let repo = base().await?;
        std::fs::write(
            repo.root().as_path().join(".git/commondir"),
            format!("{}\n", victim_git.display()),
        )?;
        foreign(repo, ".git directory with a commondir file").await
    })
    .await
}

#[tokio::test]
async fn a_gitfile_that_reaches_another_repository_through_a_symlink_and_dot_dot_is_refused()
-> TestResult {
    guard(async {
        let (_victim, victim_git) = victim().await?;
        let repo = base().await?;
        let root = repo.root().as_path();
        std::fs::remove_dir_all(root.join(".git"))?;
        std::os::unix::fs::symlink(victim_git.join("objects"), root.join("sym"))?;
        std::fs::write(root.join(".git"), "gitdir: sym/..\n")?;
        std::fs::write(root.join("gitdir"), ".git\n")?;
        foreign(repo, "gitfile gitdir: sym/..").await
    })
    .await
}

fn gitfile_target(root: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let text = std::fs::read_to_string(root.join(".git"))?;
    let target = text
        .strip_prefix("gitdir: ")
        .ok_or("not a gitfile")?
        .trim()
        .to_owned();
    Ok(root.join(target))
}

#[tokio::test]
async fn a_fifo_in_place_of_the_worktree_back_reference_is_refused_and_never_read() -> TestResult {
    guard(async {
        let main = base().await?;
        let tree = main.linked_worktree("topic").await?;
        let back = gitfile_target(tree.root().as_path())?.join("gitdir");
        let content = std::fs::read(&back)?;
        std::fs::remove_file(&back)?;
        assert!(Command::new("mkfifo").arg(&back).status()?.success());
        let feeder = back.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if let Ok(mut pipe) = OpenOptions::new().read(true).write(true).open(feeder).await {
                drop(pipe.write_all(&content).await);
            }
        });
        foreign(tree, "back reference is a FIFO").await
    })
    .await
}

#[tokio::test]
async fn a_submodule_checkout_below_a_symlinked_middle_directory_is_accepted() -> TestResult {
    guard(async {
        let source = base().await?;
        let outer = RepoFixture::new().await?;
        let url = source.root().as_path().display().to_string();
        outer
            .git(&[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                &url,
                "x/lib",
            ])
            .await?;
        outer.commit().await?;
        let links = tempfile::tempdir()?;
        let base_dir = std::fs::canonicalize(links.path())?;
        std::os::unix::fs::symlink(outer.root().as_path().join("x"), base_dir.join("mid"))?;
        let at = HostPath::parse(&base_dir.join("mid/lib"))?;
        let rig = Rig::new_at(outer, &at).await?;
        ready(rig.launch(1).await?)
    })
    .await
}

#[tokio::test]
async fn a_linked_worktree_or_submodule_below_a_symlinked_directory_is_accepted() -> TestResult {
    guard(async {
        let links = tempfile::tempdir()?;
        let base_dir = std::fs::canonicalize(links.path())?;

        let main = base().await?;
        let tree = main.linked_worktree("topic").await?;
        let parent = tree.root().as_path().parent().ok_or("no parent")?;
        std::os::unix::fs::symlink(parent, base_dir.join("via-link"))?;
        let at = HostPath::parse(&base_dir.join("via-link/tree"))?;
        ready(Rig::new_at(tree, &at).await?.launch(1).await?)?;

        let checkout = base().await?.submodule_checkout().await?;
        let outer = checkout.root().as_path().parent().ok_or("no parent")?;
        std::os::unix::fs::symlink(outer, base_dir.join("via-module"))?;
        let at = HostPath::parse(&base_dir.join("via-module/lib"))?;
        ready(Rig::new_at(checkout, &at).await?.launch(1).await?)
    })
    .await
}
