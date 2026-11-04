// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use jackin_core::{Config, Phase, ProcessRunner};
use jackin_openshell::GatewayOp;
use jackin_session::{ExportError, UnsupportedProject};
use jackin_testkit::{Fault, HostSnapshot, RepoFixture, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Sees, branch_with_commit, export, ref_of, snapshot, staging_left};

#[tokio::test]
async fn export_refuses_before_any_sandbox_or_host_write() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "work").await?;
        let session = &rig.world.session;
        let before = snapshot(&rig).await?;
        let files = handle.files();

        let absent = rig.sandbox_name(9)?;
        let missing = export(&rig, &absent).await;
        assert!(matches!(missing, Err(ExportError::NotFound(found)) if found == absent));

        for blocked in [Phase::Stopped, Phase::Error { exit_code: Some(7) }] {
            rig.world.gateway.force_phase(&name, blocked);
            let refused = export(&rig, &name).await;
            assert!(
                matches!(
                    refused,
                    Err(ExportError::NotRunning { phase, .. }) if phase == blocked
                ),
                "{refused:?}"
            );
        }
        rig.world.gateway.force_phase(&name, Phase::Ready);

        let bare = Config::empty();
        let unknown = session.export(&bare, &name, CancellationToken::new()).await;
        assert!(
            matches!(unknown, Err(ExportError::WorkspaceUnknown(_))),
            "{unknown:?}"
        );

        assert_eq!(snapshot(&rig).await?.rest, before.rest);
        assert_eq!(handle.files(), files);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());

        std::fs::remove_dir_all(rig.repo.root().as_path().join(".git"))?;
        let gone = export(&rig, &name).await;
        assert!(matches!(gone, Err(ExportError::Project(_))), "{gone:?}");
        assert_eq!(handle.files(), files);
        Ok(())
    })
    .await
}

fn listing(dir: &Path) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut names = BTreeSet::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next)? {
            let entry = entry?;
            names.insert(entry.path().strip_prefix(dir)?.display().to_string());
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    Ok(names)
}

enum Plant {
    LinkedObjects,
    LinkedReferences,
    LinkedReflog,
    Alternates,
}

#[tokio::test]
async fn a_store_tampered_with_after_launch_fails_the_export_before_index_pack() -> TestResult {
    guard(async {
        for plant in [
            Plant::LinkedObjects,
            Plant::LinkedReferences,
            Plant::LinkedReflog,
            Plant::Alternates,
        ] {
            let rig = Rig::new().await?;
            let sees = Arc::new(Sees::new(&rig));
            let runner: Arc<dyn ProcessRunner> = Arc::<Sees>::clone(&sees);
            let session = rig.session_over(runner);
            rig.run_on(&session, rig.request(1)?, CancellationToken::new())
                .await?
                .result
                .map_err(|failure| format!("{failure:?}"))?;
            let handle = rig.handle(1).await?;
            branch_with_commit(&handle, "work", "w.txt").await?;
            let attacker = tempfile::tempdir()?;
            let git_dir = rig.repo.root().as_path().join(".git");
            match plant {
                Plant::LinkedObjects => {
                    std::fs::rename(git_dir.join("objects"), attacker.path().join("objects"))?;
                    std::os::unix::fs::symlink(
                        attacker.path().join("objects"),
                        git_dir.join("objects"),
                    )?;
                }
                Plant::LinkedReferences => {
                    std::os::unix::fs::symlink(attacker.path(), git_dir.join("refs/heads/jackin"))?;
                }
                Plant::LinkedReflog => {
                    std::fs::create_dir_all(git_dir.join("logs/refs/heads"))?;
                    std::fs::rename(
                        git_dir.join("logs/refs/heads"),
                        attacker.path().join("heads"),
                    )?;
                    std::os::unix::fs::symlink(
                        attacker.path().join("heads"),
                        git_dir.join("logs/refs/heads"),
                    )?;
                }
                Plant::Alternates => {
                    std::fs::create_dir(attacker.path().join("objects"))?;
                    std::fs::write(
                        git_dir.join("objects/info/alternates"),
                        format!("{}\n", attacker.path().join("objects").display()),
                    )?;
                }
            }
            let before = listing(attacker.path())?;
            let refused = session
                .export(&rig.config, &handle.name(), CancellationToken::new())
                .await;
            match (&plant, &refused) {
                (
                    Plant::LinkedObjects,
                    Err(ExportError::Unsupported(UnsupportedProject::LinkedObjects(_))),
                )
                | (
                    Plant::LinkedReferences | Plant::LinkedReflog,
                    Err(ExportError::Unsupported(UnsupportedProject::LinkedReferences(_))),
                )
                | (
                    Plant::Alternates,
                    Err(ExportError::Unsupported(UnsupportedProject::Alternates(_))),
                ) => {}
                _ => return Err(format!("unexpected export result {refused:?}").into()),
            }
            assert!(!sees.ran("index-pack"));
            assert!(!sees.ran("update-ref"));
            assert_eq!(listing(attacker.path())?, before);
            assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        }
        Ok(())
    })
    .await
}

async fn exporting_while_the_git_directory_changes(
    rig: &Rig,
    tamper: impl FnOnce() -> Result<(), Box<dyn Error>>,
) -> Result<Result<jackin_session::ExportReport, ExportError>, Box<dyn Error>> {
    let handle = rig.handle(1).await?;
    let gateway = &rig.world.gateway;
    let held = Trigger::Op(GatewayOp::Download);
    gateway.inject(Fault::HoldAt {
        trigger: held.clone(),
    });
    let swap = async {
        if !gateway.until_held(&held).await {
            return Err("the export never reached the download".into());
        }
        let done = tamper();
        gateway.release(&held);
        done
    };
    let name = handle.name();
    let (result, swapped) = tokio::join!(export(rig, &name), swap);
    swapped?;
    Ok(result)
}

async fn victim() -> Result<RepoFixture, Box<dyn Error>> {
    let victim = RepoFixture::new().await?;
    victim.file("private.txt", b"TOP-SECRET-TOKEN\n").await?;
    victim.commit().await?;
    Ok(victim)
}

#[tokio::test]
async fn a_git_directory_replaced_by_a_link_during_the_download_never_receives_the_export()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        branch_with_commit(&handle, "work", "w.txt").await?;
        let victim = victim().await?;
        let before = HostSnapshot::take(victim.root()).await?;
        let root = rig.repo.root().as_path().to_path_buf();
        let target = victim.root().as_path().join(".git");
        let result = exporting_while_the_git_directory_changes(&rig, || {
            std::fs::rename(root.join(".git"), root.join(".jg"))?;
            std::os::unix::fs::symlink(&target, root.join(".git"))?;
            Ok(())
        })
        .await?;
        assert!(
            matches!(
                result,
                Err(ExportError::Unsupported(UnsupportedProject::ForeignGit(_)))
            ),
            "{result:?}"
        );
        let after = HostSnapshot::take(victim.root()).await?;
        assert_eq!(after.exported, before.exported);
        assert_eq!(after.rest, before.rest);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_worktree_gitfile_repointed_during_the_download_still_exports_into_its_own_repository()
-> TestResult {
    guard(async {
        let main = RepoFixture::new().await?;
        main.file("a.txt", b"alpha\n").await?;
        main.commit().await?;
        let tree = main.linked_worktree("topic").await?;
        let tree_root = tree.root().as_path().to_path_buf();
        let rig = Rig::builder().repo(tree).build().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let work = branch_with_commit(&handle, "work", "w.txt").await?;
        let victim = victim().await?;
        let before = HostSnapshot::take(victim.root()).await?;
        let target = victim.root().as_path().join(".git");
        let result = exporting_while_the_git_directory_changes(&rig, || {
            std::fs::write(
                tree_root.join(".git"),
                format!("gitdir: {}\n", target.display()),
            )?;
            Ok(())
        })
        .await?;
        let report = result?;
        assert_eq!(report.branches.len(), 1);
        let landed = HostSnapshot::take(main.root()).await?.exported;
        assert_eq!(
            landed
                .get(&ref_of(&handle.name(), "work"))
                .map(String::as_str),
            Some(work.as_str())
        );
        let after = HostSnapshot::take(victim.root()).await?;
        assert_eq!(after.exported, before.exported);
        assert_eq!(after.rest, before.rest);
        Ok(())
    })
    .await
}
