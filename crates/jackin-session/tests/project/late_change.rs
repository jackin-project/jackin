// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::time::Duration;

use jackin_core::{HostPath, RepoPath, sandbox_layout};
use jackin_session::{LaunchError, LaunchStep, UnsupportedProject};
use jackin_testkit::RepoFixture;

use super::oracle::arrived;
use super::support::{Outcome, Rig, TestResult, failure, guard, launch_id, ready, tree_of};

#[derive(Clone, Copy)]
enum Change {
    Nothing,
    NestedRepository,
    Commit,
    SymlinkParent,
}

async fn base() -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.commit().await?;
    Ok(repo)
}

async fn during_the_lock_wait(rig: &Rig, change: Change) -> Result<Outcome, Box<dyn Error>> {
    rig.world.fs.create_dir_all(rig.world.cache_dir()).await?;
    let lock = HostPath::resolve(
        Path::new(&format!("upload-{}.lock", launch_id(1))),
        rig.world.cache_dir(),
    )?;
    let held = rig.world.fs.lock(&lock).await?;
    let peer = async {
        while !rig
            .world
            .clock
            .slept()
            .contains(&Duration::from_millis(100))
        {
            tokio::task::yield_now().await;
        }
        match change {
            Change::Nothing => {}
            Change::NestedRepository => {
                rig.repo.git(&["init", "-q", "vendor/dep"]).await?;
                rig.repo.file("vendor/dep/x.txt", b"nested\n").await?;
            }
            Change::Commit => {
                rig.repo.file("b.txt", b"beta\n").await?;
                rig.repo.commit().await?;
            }
            Change::SymlinkParent => {
                let root = rig.repo.root().as_path();
                std::fs::remove_dir_all(root.join("d"))?;
                std::os::unix::fs::symlink(root.join("real"), root.join("d"))?;
            }
        }
        drop(held);
        Ok::<(), Box<dyn Error>>(())
    };
    let (outcome, peer) = tokio::join!(rig.launch(1), peer);
    peer?;
    outcome
}

#[tokio::test]
async fn a_nested_repository_created_after_the_early_check_is_refused_at_the_upload() -> TestResult
{
    guard(async {
        let rig = Rig::new(base().await?).await?;
        let outcome = during_the_lock_wait(&rig, Change::NestedRepository).await?;
        let failed = failure(outcome)?;
        assert_eq!(failed.step, LaunchStep::Upload);
        assert!(
            matches!(
                *failed.error,
                LaunchError::Unsupported(UnsupportedProject::NestedRepositories(_))
            ),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        let uploaded = handle
            .files()
            .keys()
            .filter(|path| path.as_str().starts_with("/sandbox/project"))
            .count();
        assert_eq!(uploaded, 0);
        assert!(!handle.root().join("project/vendor").exists());
        assert!(!handle.root().join("project/.git").exists());
        assert!(
            handle
                .file(&sandbox_layout::uploaded_marker(&launch_id(1)))
                .is_none()
        );
        let cache = tree_of(rig.world.cache_dir().as_path())?;
        assert!(
            cache
                .iter()
                .all(|name| !name.contains("upload-") || name.contains(".lock")),
            "{cache:?}"
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_commit_made_after_the_early_check_is_the_head_that_arrives() -> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        ready(during_the_lock_wait(&rig, Change::Commit).await?)?;
        let handle = rig.handle(1).await?;
        arrived(rig.repo.root().as_path(), &handle.root(), &[])?;

        let control = Rig::new(base().await?).await?;
        ready(during_the_lock_wait(&control, Change::Nothing).await?)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_directory_replaced_by_a_symlink_after_the_early_check_is_refused_at_the_upload()
-> TestResult {
    guard(async {
        let repo = base().await?;
        repo.file("d/f.txt", b"inside\n").await?;
        repo.file("real/f.txt", b"other\n").await?;
        repo.commit().await?;
        let rig = Rig::new(repo).await?;
        let outcome = during_the_lock_wait(&rig, Change::SymlinkParent).await?;
        let failed = failure(outcome)?;
        assert_eq!(failed.step, LaunchStep::Upload);
        assert!(
            matches!(
                &*failed.error,
                LaunchError::Unsupported(UnsupportedProject::SymlinkedPath(paths))
                    if paths.iter().map(RepoPath::as_str).collect::<Vec<_>>() == ["d/f.txt"]
            ),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        assert!(
            handle
                .files()
                .keys()
                .all(|path| !path.as_str().starts_with("/sandbox/project"))
        );
        assert!(!handle.root().join("project/d").exists());
        Ok(())
    })
    .await
}
