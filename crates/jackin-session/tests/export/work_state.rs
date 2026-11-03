// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{BranchName, Config, Phase, SandboxName};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{PurgeError, PurgeMode, Unexported, UnknownReason, WorkState};
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};
use tokio_util::sync::CancellationToken;

use super::support::{export, ref_of, state};

fn unexported(
    uncommitted: usize,
    branches: &[&str],
) -> Result<WorkState, Box<dyn std::error::Error>> {
    let branches = branches
        .iter()
        .map(|branch| BranchName::parse(branch))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WorkState::Unexported(Unexported {
        uncommitted,
        branches,
        detached: None,
        worktrees: Vec::new(),
        unusable: Vec::new(),
    }))
}

#[tokio::test]
async fn a_fresh_launch_is_exported_and_only_real_changes_make_it_unexported() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.repo.file("target/ignored.bin", b"ignored\n").await?;
        rig.launch(1).await?;
        let first = rig.sandbox_name(1)?;
        assert_eq!(state(&rig, &first).await, WorkState::Exported);
        let ignored = rig.handle(1).await?.root().join("project/target/x");
        tokio::fs::create_dir_all(ignored.parent().ok_or("no parent")?).await?;
        tokio::fs::write(&ignored, b"built\n").await?;
        assert_eq!(state(&rig, &first).await, WorkState::Exported);

        rig.repo.file("README.md", b"edited\n").await?;
        rig.repo.file("notes.txt", b"untracked\n").await?;
        rig.launch(2).await?;
        let handle = rig.handle(2).await?;
        let dirty = handle.name();
        assert_eq!(state(&rig, &dirty).await, unexported(2, &[])?);
        handle
            .commit_in_project("c.txt", b"c", "commit all")
            .await?;
        assert_eq!(state(&rig, &dirty).await, unexported(0, &["main"])?);
        export(&rig, &dirty).await?;
        assert_eq!(state(&rig, &dirty).await, WorkState::Exported);
        handle.commit_in_project("d.txt", b"d", "more").await?;
        assert_eq!(state(&rig, &dirty).await, unexported(0, &["main"])?);

        handle.git(&["checkout", "-q", "-b", "side-work"]).await?;
        export(&rig, &dirty).await?;
        assert_eq!(state(&rig, &dirty).await, WorkState::Exported);
        std::fs::write(handle.root().join("project/README.md"), b"stash me\n")?;
        handle.git(&["stash", "push", "-q"]).await?;
        assert_eq!(state(&rig, &dirty).await, unexported(1, &[])?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_untracked_file_alone_counts_as_uncommitted() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.repo.file("only-untracked.txt", b"u\n").await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        assert_eq!(state(&rig, &name).await, unexported(1, &[])?);
        Ok(())
    })
    .await
}

async fn unpushed_refs(rig: &Rig, name: &SandboxName) -> Option<usize> {
    match state(rig, name).await {
        WorkState::Unexported(found) => Some(found.unpushed()),
        WorkState::Exported | WorkState::Unknown(_) => None,
    }
}

#[tokio::test]
async fn unpushed_counts_each_ref_whose_commits_the_host_lacks() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        assert_eq!(unpushed_refs(&rig, &name).await, None);
        handle.commit_in_project("a.txt", b"a", "on main").await?;
        assert_eq!(unpushed_refs(&rig, &name).await, Some(1));
        handle.git(&["checkout", "-q", "-b", "side-work"]).await?;
        handle.commit_in_project("b.txt", b"b", "on side").await?;
        assert_eq!(unpushed_refs(&rig, &name).await, Some(2));
        export(&rig, &name).await?;
        assert_eq!(unpushed_refs(&rig, &name).await, None);
        Ok(())
    })
    .await
}

async fn unknown(rig: &Rig, name: &SandboxName) -> Option<UnknownReason> {
    match state(rig, name).await {
        WorkState::Unknown(reason) => Some(reason),
        WorkState::Exported | WorkState::Unexported(_) => None,
    }
}

#[tokio::test]
async fn what_jackin_cannot_inspect_is_unknown_and_a_forged_supervisor_changes_nothing()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let gateway = &rig.world.gateway;
        for phase in [
            Phase::Stopped,
            Phase::Error { exit_code: Some(7) },
            Phase::Completed { exit_code: 0 },
        ] {
            gateway.force_phase(&name, phase);
            assert_eq!(
                unknown(&rig, &name).await,
                Some(UnknownReason::NotRunning(phase))
            );
        }
        gateway.force_phase(&name, Phase::Ready);

        let session = &rig.world.session;
        let bare = session.work_state(&Config::empty(), &name).await;
        assert!(
            matches!(bare, WorkState::Unknown(UnknownReason::WorkspaceUnknown(_))),
            "{bare:?}"
        );

        gateway.inject(Fault::FailAt {
            trigger: Trigger::Program(jackin_core::sandbox_layout::GIT.clone()),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        assert_eq!(
            unknown(&rig, &name).await,
            Some(UnknownReason::InspectionFailed)
        );
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        std::fs::remove_file(handle.root().join(".jackin/run/state"))?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        handle.commit_in_project("a.txt", b"a", "work").await?;
        let expected = unexported(0, &["main"])?;
        for text in ["exited 1 0\n", "garbage\n", "waiting\n"] {
            std::fs::create_dir_all(handle.root().join(".jackin/run"))?;
            std::fs::write(handle.root().join(".jackin/run/state"), text)?;
            assert_eq!(state(&rig, &name).await, expected, "{text:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_launch_that_failed_before_the_upload_has_no_repository() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.world.gateway.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            error: GatewayError::Protocol {
                message: "injected".to_owned(),
            },
        });
        let failed = rig.launch_with(1, CancellationToken::new()).await?;
        assert!(failed.is_err(), "{failed:?}");
        let name = rig.sandbox_name(1)?;
        assert_eq!(
            unknown(&rig, &name).await,
            Some(UnknownReason::NoRepository)
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_host_ref_deleted_or_a_branch_advanced_after_the_export_lists_the_branch() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("a.txt", b"a", "one").await?;
        export(&rig, &name).await?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        rig.repo
            .git(&["update-ref", "-d", &ref_of(&name, "main")])
            .await?;
        assert_eq!(state(&rig, &name).await, unexported(0, &["main"])?);
        export(&rig, &name).await?;
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        handle.commit_in_project("b.txt", b"b", "two").await?;
        assert_eq!(state(&rig, &name).await, unexported(0, &["main"])?);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_corrupt_sandbox_index_still_exports_the_branches_but_the_work_state_is_unknown()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let tip = handle.commit_in_project("a.txt", b"a", "work").await?;
        std::fs::write(handle.root().join("project/.git/index"), b"not an index")?;

        export(&rig, &name).await?;

        let landed = rig.repo.git(&["rev-parse", &ref_of(&name, "main")]).await?;
        assert_eq!(landed.trim(), tip.as_str());
        assert_eq!(
            state(&rig, &name).await,
            WorkState::Unknown(UnknownReason::InspectionFailed)
        );
        let purge = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded)
            .await;
        assert!(
            matches!(purge, Err(PurgeError::WorkNotExported { .. })),
            "{purge:?}"
        );
        assert!(rig.world.gateway.sandbox(&name).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_stash_counts_from_its_ref_so_an_expired_reflog_still_blocks_a_guarded_purge()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        assert_eq!(state(&rig, &name).await, WorkState::Exported);

        std::fs::write(handle.root().join("project/README.md"), b"first\n")?;
        handle.git(&["stash", "push", "-q"]).await?;
        assert_eq!(state(&rig, &name).await, unexported(1, &[])?);

        std::fs::write(handle.root().join("project/README.md"), b"second\n")?;
        handle.git(&["stash", "push", "-q"]).await?;
        assert_eq!(state(&rig, &name).await, unexported(2, &[])?);

        handle
            .git(&["reflog", "expire", "--expire=now", "--all"])
            .await?;
        assert_eq!(handle.git(&["stash", "list"]).await?, "");
        assert_eq!(state(&rig, &name).await, unexported(1, &[])?);

        let purge = rig
            .world
            .session
            .purge(&rig.config, &name, PurgeMode::Guarded)
            .await;
        assert!(
            matches!(purge, Err(PurgeError::WorkNotExported { .. })),
            "{purge:?}"
        );
        assert!(rig.world.gateway.sandbox(&name).is_some());
        Ok(())
    })
    .await
}
