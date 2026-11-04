// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use jackin_core::{ProcessRunner, sandbox_layout};
use jackin_session::{LaunchError, LaunchStep, UnsupportedProject};
use jackin_testkit::RepoFixture;
use tokio_util::sync::CancellationToken;

use super::support::{
    Point, Rig, Tamper, TestResult, assert_nothing_created, failure, guard, launch_id, unsupported,
};

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

#[tokio::test]
async fn alternates_added_while_staging_are_refused() -> TestResult {
    guard(async {
        for occurrence in [1, 2] {
            let other = RepoFixture::new().await?;
            other.file("private.txt", b"TOP-SECRET-TOKEN\n").await?;
            other.commit().await?;
            let repo = base().await?;
            let file = repo.root().as_path().join(".git/objects/info/alternates");
            let listed = other.root().as_path().join(".git/objects");
            let rig = Rig::new(repo).await?;
            let tamper = Tamper::new(&rig, Point::AfterBundle(occurrence), {
                let file = file.clone();
                move || std::fs::write(&file, format!("{}\n", listed.display()))
            });
            let runner: Arc<dyn ProcessRunner> = Arc::<Tamper>::clone(&tamper);
            let outcome = rig.launch_over(1, runner, CancellationToken::new()).await?;
            tamper.done()?;
            let failed = failure(outcome)?;
            match *failed.error {
                LaunchError::Unsupported(UnsupportedProject::Alternates(named)) => {
                    assert_eq!(named.as_path(), file, "bundle {occurrence}");
                }
                other => {
                    return Err(
                        format!("bundle {occurrence}: expected Alternates, saw {other:?}").into(),
                    );
                }
            }
            if occurrence == 1 {
                assert_nothing_created(&rig)?;
            } else {
                assert_eq!(failed.step, LaunchStep::Upload);
                let handle = rig.handle(1).await?;
                assert!(
                    handle
                        .file(&sandbox_layout::uploaded_marker(&launch_id(1)))
                        .is_none()
                );
                let bundle =
                    sandbox_layout::BUNDLE_DIR.join(&format!("upload-{}.bundle", launch_id(1)))?;
                assert!(handle.file(&bundle).is_none());
            }
        }
        Ok(())
    })
    .await
}
