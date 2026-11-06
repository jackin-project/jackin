// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_session::{PurgeError, PurgeMode, Unexported, WorkState};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{export, exports, ref_of, snapshot};

#[tokio::test]
async fn a_detached_head_commit_is_unexported_until_it_is_exported_as_the_head_ref() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        assert_eq!(
            session
                .work_state(
                    &rig.config,
                    &name,
                    &tokio_util::sync::CancellationToken::new()
                )
                .await,
            WorkState::Exported
        );

        handle.git(&["checkout", "-q", "--detach"]).await?;
        let commit = handle
            .commit_in_project("d.txt", b"d", "detached work")
            .await?;
        assert_eq!(
            session
                .work_state(
                    &rig.config,
                    &name,
                    &tokio_util::sync::CancellationToken::new()
                )
                .await,
            WorkState::Unexported(Unexported {
                uncommitted: 0,
                branches: Vec::new(),
                detached: Some(commit.clone()),
                worktrees: Vec::new(),
                unusable: Vec::new(),
            })
        );

        let report = export(&rig, &name).await?;

        assert!(report.branches.is_empty());
        let detached = report.detached.ok_or("no detached head exported")?;
        assert_eq!(
            (detached.tip.clone(), detached.host_ref),
            (commit.clone(), ref_of(&name, "HEAD"))
        );
        let refs = exports(&snapshot(&rig).await?);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs.get(&ref_of(&name, "HEAD")).map(String::as_str),
            Some(commit.as_str())
        );
        assert_eq!(
            session
                .work_state(
                    &rig.config,
                    &name,
                    &tokio_util::sync::CancellationToken::new()
                )
                .await,
            WorkState::Exported
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_launched_from_a_detached_host_head_is_exported_and_exports_nothing() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.repo.git(&["checkout", "-q", "--detach"]).await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        assert_eq!(
            rig.world
                .session
                .work_state(
                    &rig.config,
                    &name,
                    &tokio_util::sync::CancellationToken::new()
                )
                .await,
            WorkState::Exported
        );

        let report = export(&rig, &name).await?;

        assert!(report.bundle.is_none() && report.branches.is_empty() && report.detached.is_none());
        assert!(exports(&snapshot(&rig).await?).is_empty());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_head_symbolic_to_a_ref_outside_heads_is_unexported_work_until_it_is_exported()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let session = &rig.world.session;
        for (id, target) in [
            (1_u8, "refs/remotes/origin/x"),
            (2, "refs/tags/t"),
            (3, "refs/notes/n"),
        ] {
            rig.launch(id).await?;
            let handle = rig.handle(id).await?;
            let name = handle.name();
            handle.git(&["update-ref", target, "HEAD"]).await?;
            handle.git(&["symbolic-ref", "HEAD", target]).await?;
            assert_eq!(
                session
                    .work_state(
                        &rig.config,
                        &name,
                        &tokio_util::sync::CancellationToken::new()
                    )
                    .await,
                WorkState::Exported,
                "{target}"
            );

            let commit = handle
                .commit_in_project("agent.txt", b"only copy", "agent work")
                .await?;
            assert_eq!(
                handle.git(&["rev-parse", target]).await?.trim(),
                commit.as_str(),
                "{target}"
            );
            assert_eq!(
                session
                    .work_state(
                        &rig.config,
                        &name,
                        &tokio_util::sync::CancellationToken::new()
                    )
                    .await,
                WorkState::Unexported(Unexported {
                    uncommitted: 0,
                    branches: Vec::new(),
                    detached: Some(commit.clone()),
                    worktrees: Vec::new(),
                    unusable: Vec::new(),
                }),
                "{target}"
            );
            let purge = session
                .purge(
                    &rig.config,
                    &name,
                    PurgeMode::Guarded,
                    &tokio_util::sync::CancellationToken::new(),
                )
                .await;
            assert!(
                matches!(purge, Err(PurgeError::WorkNotExported { .. })),
                "{target}: {purge:?}"
            );
            assert!(rig.world.gateway.sandbox(&name).is_some(), "{target}");

            let report = export(&rig, &name).await?;

            let detached = report.detached.ok_or("no detached head exported")?;
            assert_eq!(
                (detached.tip, detached.host_ref),
                (commit.clone(), ref_of(&name, "HEAD")),
                "{target}"
            );
            assert_eq!(
                session
                    .work_state(
                        &rig.config,
                        &name,
                        &tokio_util::sync::CancellationToken::new()
                    )
                    .await,
                WorkState::Exported,
                "{target}"
            );
        }
        Ok(())
    })
    .await
}
