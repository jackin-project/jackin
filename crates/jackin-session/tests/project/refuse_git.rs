// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use jackin_core::{HostPath, ProcessRunner};
use jackin_session::UnsupportedProject;
use jackin_testkit::RepoFixture;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::support::{
    Point, Rig, Tamper, TestResult, assert_nothing_created, failure, guard, ready, unsupported,
};

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

async fn with_gitfile(target: &str) -> Result<RepoFixture, Box<dyn Error>> {
    let repo = base().await?;
    let root = repo.root().as_path();
    std::fs::remove_dir_all(root.join(".git"))?;
    std::fs::write(root.join(".git"), format!("gitdir: {target}\n"))?;
    Ok(repo)
}

#[tokio::test]
async fn a_gitfile_that_does_not_point_back_at_the_project_is_refused() -> TestResult {
    guard(async {
        let (victim, victim_git) = victim().await?;
        foreign(
            with_gitfile(&victim_git.display().to_string()).await?,
            "gitfile naming another repository",
        )
        .await?;

        let up = "../".repeat(victim_git.components().count() + 8);
        let relative = format!(
            "{up}{}",
            victim_git.display().to_string().trim_start_matches('/')
        );
        foreign(
            with_gitfile(&relative).await?,
            "gitfile with a relative path to another repository",
        )
        .await?;

        let worktree = victim.linked_worktree("lw").await?;
        let worktree_gitdir = gitfile_target(worktree.root().as_path())?;
        foreign(
            with_gitfile(&worktree_gitdir.display().to_string()).await?,
            "gitfile naming a linked worktree gitdir that points back elsewhere",
        )
        .await?;

        let checkout = victim.submodule_checkout().await?;
        let module_gitdir = gitfile_target(checkout.root().as_path())?;
        foreign(
            with_gitfile(&module_gitdir.display().to_string()).await?,
            "gitfile naming a submodule gitdir whose work tree is elsewhere",
        )
        .await?;

        let separate = base().await?;
        let moved = victim_git.parent().ok_or("no parent")?.join("separate.git");
        std::fs::rename(separate.root().as_path().join(".git"), &moved)?;
        std::fs::write(
            separate.root().as_path().join(".git"),
            format!("gitdir: {}\n", moved.display()),
        )?;
        foreign(separate, "separate git directory").await
    })
    .await
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

fn swap_git_directory(root: &Path, victim_git: &Path, gitfile: bool) -> std::io::Result<()> {
    std::fs::rename(root.join(".git"), root.join(".jg"))?;
    if gitfile {
        std::fs::write(
            root.join(".git"),
            format!("gitdir: {}\n", victim_git.display()),
        )
    } else {
        std::os::unix::fs::symlink(victim_git, root.join(".git"))
    }
}

#[tokio::test]
async fn a_git_directory_swapped_after_open_is_not_followed() -> TestResult {
    guard(async {
        for gitfile in [true, false] {
            let (_victim, victim_git) = victim().await?;
            let repo = base().await?;
            let root = repo.root().as_path().to_path_buf();
            let rig = Rig::new(repo).await?;
            let tamper = Tamper::new(&rig, Point::AfterOpen, move || {
                swap_git_directory(&root, &victim_git, gitfile)
            });
            let runner: Arc<dyn ProcessRunner> = Arc::<Tamper>::clone(&tamper);
            let outcome = rig.launch_over(1, runner, CancellationToken::new()).await?;
            tamper.done()?;
            let failed = failure(outcome)?;
            assert!(failed.sandbox.is_none(), "gitfile {gitfile}: {failed:?}");
            assert_nothing_created(&rig)?;
        }
        Ok(())
    })
    .await
}

async fn shallow_clone() -> Result<RepoFixture, Box<dyn Error>> {
    let source = base().await?;
    source.file("a.txt", b"second\n").await?;
    source.commit().await?;
    source.clone_of(&["--depth", "1"]).await
}

#[tokio::test]
async fn a_shallow_file_that_is_a_symlink_is_refused_and_a_regular_one_arrives() -> TestResult {
    guard(async {
        let regular = Rig::new(shallow_clone().await?).await?;
        ready(regular.launch(1).await?)?;
        let handle = regular.handle(1).await?;
        assert_eq!(
            std::fs::read(handle.root().join("project/.git/shallow"))?,
            std::fs::read(regular.repo.root().as_path().join(".git/shallow"))?
        );

        let repo = shallow_clone().await?;
        let shallow = repo.root().as_path().join(".git/shallow");
        let outside = tempfile::tempdir()?;
        std::fs::copy(&shallow, outside.path().join("shallow"))?;
        std::fs::remove_file(&shallow)?;
        std::os::unix::fs::symlink(outside.path().join("shallow"), &shallow)?;
        let rig = Rig::new(repo).await?;
        match unsupported(rig.launch(1).await?)? {
            UnsupportedProject::LinkedShallow(named) => assert_eq!(named.as_path(), shallow),
            other => return Err(format!("expected LinkedShallow, saw {other:?}").into()),
        }
        assert_nothing_created(&rig)
    })
    .await
}

const OVER_ANY_CAP: u64 = 64 << 20;

fn overfill(file: &Path, name: &str) -> Result<u64, Box<dyn Error>> {
    if name == "shallow" {
        let line = std::fs::read(file)?;
        let bytes = line.repeat(130_000);
        std::fs::write(file, &bytes)?;
        Ok(u64::try_from(bytes.len())?)
    } else {
        drop(std::fs::remove_file(file));
        std::fs::File::create(file)?.set_len(OVER_ANY_CAP)?;
        Ok(OVER_ANY_CAP)
    }
}

#[tokio::test]
async fn an_oversized_info_exclude_or_attributes_is_refused_and_a_small_exclude_is_honored()
-> TestResult {
    guard(async {
        for name in ["info/exclude", "info/attributes", "shallow"] {
            let repo = if name == "shallow" {
                shallow_clone().await?
            } else {
                base().await?
            };
            let file = repo.root().as_path().join(".git").join(name);
            let size = overfill(&file, name)?;
            let rig = Rig::new(repo).await?;
            match unsupported(rig.launch(1).await?)? {
                UnsupportedProject::OversizedMetadata { file: named, limit } => {
                    assert_eq!(named.as_path(), file, "{name}");
                    assert!(
                        u64::try_from(limit)? < size,
                        "{name}: the cap {limit} is not a cap"
                    );
                }
                other => {
                    return Err(format!("{name}: expected OversizedMetadata, saw {other:?}").into());
                }
            }
            assert_nothing_created(&rig)?;
        }

        let repo = base().await?;
        repo.file("noise.log", b"noise\n").await?;
        std::fs::write(repo.root().as_path().join(".git/info/exclude"), "*.log\n")?;
        let rig = Rig::new(repo).await?;
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        assert!(!handle.root().join("project/noise.log").exists());
        Ok(())
    })
    .await
}
