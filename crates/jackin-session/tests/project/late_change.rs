// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::path::Path;
use std::time::Duration;

use jackin_core::{HostPath, RepoPath, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_session::{LaunchError, LaunchStep, UnsupportedProject};
use jackin_testkit::{Fault, RepoFixture, Trigger};

use super::oracle::arrived;
use super::support::{Outcome, Rig, TestResult, failure, guard, launch_id, ready, tree_of};

#[derive(Clone, Copy)]
enum Change {
    Nothing,
    NestedRepository,
    Commit,
    SymlinkParent,
    RootReplaced,
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
            Change::RootReplaced => {
                let root = rig.repo.root().as_path();
                let parked = root.with_extension("parked");
                std::fs::rename(root, &parked)?;
                let copied = std::process::Command::new("/bin/cp")
                    .arg("-a")
                    .arg(&parked)
                    .arg(root)
                    .status()?;
                assert!(copied.success(), "cp failed");
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

#[derive(Clone, Copy)]
enum UploadChange {
    Nothing,
    Add,
    Remove,
    EditContent,
    AddIgnored,
    EmptyCommit,
    Deepen,
}

async fn upload_base() -> Result<RepoFixture, Box<dyn Error>> {
    let repo = RepoFixture::new().await?;
    repo.file(".gitignore", b"*.log\n").await?;
    repo.file("a.txt", b"alpha\n").await?;
    repo.file("b.txt", b"beta\n").await?;
    repo.commit().await?;
    Ok(repo)
}

async fn during_the_upload(rig: &Rig, change: UploadChange) -> Result<Outcome, Box<dyn Error>> {
    let trigger = Trigger::Op(GatewayOp::UploadTree);
    rig.world.gateway.inject(Fault::HoldAt {
        trigger: trigger.clone(),
    });
    let peer = async {
        if !rig.world.gateway.until_held(&trigger).await {
            return Err("the upload never reached the gateway".into());
        }
        let root = rig.repo.root().as_path();
        match change {
            UploadChange::Nothing => {}
            UploadChange::Add => std::fs::write(root.join("late-arrival.txt"), b"late\n")?,
            UploadChange::Remove => std::fs::remove_file(root.join("b.txt"))?,
            UploadChange::EditContent => std::fs::write(root.join("a.txt"), b"edited\n")?,
            UploadChange::AddIgnored => std::fs::write(root.join("debug.log"), b"noise\n")?,
            UploadChange::EmptyCommit => {
                rig.repo
                    .git(&["commit", "-q", "--allow-empty", "-m", "moved"])
                    .await?;
            }
            UploadChange::Deepen => {
                rig.repo.git(&["fetch", "-q", "--deepen=1"]).await?;
            }
        }
        rig.world.gateway.release(&trigger);
        Ok::<(), Box<dyn Error>>(())
    };
    let (outcome, peer) = tokio::join!(rig.launch(1), peer);
    peer?;
    outcome
}

async fn refused_at_the_upload(
    change: UploadChange,
) -> Result<(Vec<String>, Vec<String>), Box<dyn Error>> {
    let rig = Rig::new(upload_base().await?).await?;
    let failed = failure(during_the_upload(&rig, change).await?)?;
    assert_eq!(failed.step, LaunchStep::Upload);
    let LaunchError::Unsupported(UnsupportedProject::ChangedDuringUpload { added, removed }) =
        *failed.error
    else {
        return Err("expected ChangedDuringUpload".into());
    };
    let handle = rig.handle(1).await?;
    assert!(
        handle
            .file(&sandbox_layout::uploaded_marker(&launch_id(1)))
            .is_none()
    );
    let names = |paths: Vec<RepoPath>| paths.iter().map(|path| path.as_str().to_owned()).collect();
    Ok((names(added), names(removed)))
}

#[tokio::test]
async fn a_file_added_between_staging_and_the_upload_fails_the_launch() -> TestResult {
    guard(async {
        let (added, removed) = refused_at_the_upload(UploadChange::Add).await?;
        assert_eq!(added, ["late-arrival.txt"]);
        assert!(removed.is_empty(), "{removed:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_tracked_file_removed_between_staging_and_the_upload_fails_the_launch() -> TestResult {
    guard(async {
        let (added, removed) = refused_at_the_upload(UploadChange::Remove).await?;
        assert!(added.is_empty(), "{added:?}");
        assert_eq!(removed, ["b.txt"]);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_edit_or_an_ignored_file_during_the_upload_does_not_fail_the_launch() -> TestResult {
    guard(async {
        for change in [
            UploadChange::Nothing,
            UploadChange::EditContent,
            UploadChange::AddIgnored,
        ] {
            let rig = Rig::new(upload_base().await?).await?;
            ready(during_the_upload(&rig, change).await?)?;
            let handle = rig.handle(1).await?;
            let uploaded = std::fs::read(handle.root().join("project/a.txt"))?;
            let host = std::fs::read(rig.repo.root().as_path().join("a.txt"))?;
            assert_eq!(uploaded, host);
            assert!(!handle.root().join("project/debug.log").exists());
        }
        Ok(())
    })
    .await
}

async fn refused_for_history(rig: &Rig, change: UploadChange) -> TestResult {
    let failed = failure(during_the_upload(rig, change).await?)?;
    assert_eq!(failed.step, LaunchStep::Upload);
    assert!(
        matches!(
            *failed.error,
            LaunchError::Unsupported(UnsupportedProject::HistoryChangedDuringUpload)
        ),
        "{:?}",
        failed.error
    );
    let handle = rig.handle(1).await?;
    assert!(
        handle
            .file(&sandbox_layout::uploaded_marker(&launch_id(1)))
            .is_none()
    );
    Ok(())
}

#[tokio::test]
async fn a_commit_made_during_the_upload_fails_the_launch() -> TestResult {
    guard(async {
        let rig = Rig::new(upload_base().await?).await?;
        refused_for_history(&rig, UploadChange::EmptyCommit).await
    })
    .await
}

#[tokio::test]
async fn a_shallow_boundary_moved_during_the_upload_fails_the_launch() -> TestResult {
    guard(async {
        let source = upload_base().await?;
        source.file("c.txt", b"gamma\n").await?;
        source.commit().await?;
        source.file("d.txt", b"delta\n").await?;
        source.commit().await?;
        let shallow = source.clone_of(&["--depth", "1"]).await?;
        let rig = Rig::new(shallow).await?;
        refused_for_history(&rig, UploadChange::Deepen).await
    })
    .await
}

#[tokio::test]
async fn a_project_directory_replaced_by_a_copy_after_it_was_chosen_is_refused() -> TestResult {
    guard(async {
        let rig = Rig::new(base().await?).await?;
        let outcome = during_the_lock_wait(&rig, Change::RootReplaced).await?;
        let failed = failure(outcome)?;
        assert_eq!(failed.step, LaunchStep::Upload);
        assert!(
            matches!(
                *failed.error,
                LaunchError::Unsupported(UnsupportedProject::RootReplaced(_))
            ),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        assert!(!handle.root().join("project/.git").exists());
        Ok(())
    })
    .await
}
