// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::{Path, PathBuf};

use jackin_session::UnsupportedProject;
use jackin_testkit::RepoFixture;

use super::support::{Rig, TestResult, assert_nothing_created, guard, unsupported};

async fn base() -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Ok(repo)
}

async fn linked(repo: RepoFixture, named: &Path) -> TestResult {
    let rig = Rig::new(repo).await?;
    match unsupported(rig.launch(1).await?)? {
        UnsupportedProject::LinkedObjects(found) => assert_eq!(found.as_path(), named),
        other => return Err(format!("expected LinkedObjects, saw {other:?}").into()),
    }
    assert_nothing_created(&rig)
}

fn first_entry(dir: &Path, want_dir: bool) -> Result<PathBuf, Box<dyn Error>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type()?.is_dir() == want_dir && name != "info" && name != "pack" {
            found.push(entry.path());
        }
    }
    found.sort();
    found.into_iter().next().ok_or_else(|| "no entry".into())
}

fn move_behind_a_symlink(entry: &Path, to: &Path) -> TestResult {
    std::fs::rename(entry, to)?;
    std::os::unix::fs::symlink(to, entry)?;
    Ok(())
}

#[tokio::test]
async fn a_symlinked_pack_directory_is_refused() -> TestResult {
    guard(async {
        let repo = base().await?;
        let git_dir = repo.root().as_path().join(".git");
        let pack = git_dir.join("objects/pack");
        move_behind_a_symlink(&pack, &git_dir.join("pack.real"))?;
        linked(repo, &pack).await
    })
    .await
}

#[tokio::test]
async fn a_symlinked_fan_out_directory_is_refused() -> TestResult {
    guard(async {
        let repo = base().await?;
        let git_dir = repo.root().as_path().join(".git");
        let fan = first_entry(&git_dir.join("objects"), true)?;
        move_behind_a_symlink(&fan, &git_dir.join("fan.real"))?;
        linked(repo, &fan).await
    })
    .await
}

#[tokio::test]
async fn a_symlinked_loose_object_is_refused() -> TestResult {
    guard(async {
        let repo = base().await?;
        let git_dir = repo.root().as_path().join(".git");
        let fan = first_entry(&git_dir.join("objects"), true)?;
        let loose = first_entry(&fan, false)?;
        move_behind_a_symlink(&loose, &git_dir.join("loose.real"))?;
        linked(repo, &loose).await
    })
    .await
}

#[tokio::test]
async fn a_symlinked_pack_file_is_refused() -> TestResult {
    guard(async {
        let repo = base().await?;
        repo.git(&["repack", "-a", "-d", "-q"]).await?;
        let git_dir = repo.root().as_path().join(".git");
        let mut packs = Vec::new();
        for entry in std::fs::read_dir(git_dir.join("objects/pack"))? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "pack")
            {
                packs.push(path);
            }
        }
        let pack = packs.pop().ok_or("no pack file")?;
        move_behind_a_symlink(&pack, &git_dir.join("moved.pack"))?;
        linked(repo, &pack).await
    })
    .await
}

#[tokio::test]
async fn a_pack_entry_that_is_not_a_directory_is_refused() -> TestResult {
    guard(async {
        let repo = base().await?;
        let pack = repo.root().as_path().join(".git/objects/pack");
        std::fs::remove_dir_all(&pack)?;
        std::fs::write(&pack, b"not a directory")?;
        linked(repo, &pack).await
    })
    .await
}

#[tokio::test]
async fn a_linked_worktree_of_a_repository_that_borrows_objects_is_refused() -> TestResult {
    guard(async {
        let other = RepoFixture::new().await?;
        other.file("private.txt", b"TOP-SECRET-TOKEN\n").await?;
        other.commit().await?;
        let head = other.git(&["rev-parse", "HEAD"]).await?.trim().to_owned();
        let main = base().await?;
        let tree = main.linked_worktree("topic").await?;
        let git_dir = main.root().as_path().join(".git");
        let file = git_dir.join("objects/info/alternates");
        let listed = other.root().as_path().join(".git/objects");
        std::fs::write(&file, format!("{}\n", listed.display()))?;
        std::fs::create_dir_all(git_dir.join("refs/remotes/o"))?;
        std::fs::write(git_dir.join("refs/remotes/o/x"), format!("{head}\n"))?;
        let rig = Rig::new(tree).await?;
        match unsupported(rig.launch(1).await?)? {
            UnsupportedProject::Alternates(named) => assert_eq!(named.as_path(), file),
            other => return Err(format!("expected Alternates, saw {other:?}").into()),
        }
        assert_nothing_created(&rig)
    })
    .await
}
