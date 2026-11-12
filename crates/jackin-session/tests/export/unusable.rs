// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{HostPath, TerminalText};
use jackin_openshell::GatewayOp;
use jackin_session::{ExportError, PurgeError, PurgeMode, Unexported, WorkState};
use jackin_testkit::{Fault, Rig, TestResult, Trigger, guard};

use super::support::{branch_with_commit, export, exports, ref_of, snapshot, state};

#[tokio::test]
async fn a_sandbox_branch_under_jackin_fails_the_export_and_is_never_exported() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        branch_with_commit(&handle, "good", "g.txt").await?;
        handle.git(&["branch", "jackin/evil"]).await?;
        let before = snapshot(&rig).await?;

        let refused = export(&rig, &name).await;

        assert!(
            matches!(
                &refused,
                Err(ExportError::UnusableBranch { branch }) if branch.as_str() == "jackin/evil"
            ),
            "{refused:?}"
        );
        let after = snapshot(&rig).await?;
        assert!(exports(&after).is_empty());
        assert_eq!(after.rest, before.rest);
        let state = session.work_state(&rig.config, &name).await;
        assert_eq!(
            state,
            WorkState::Unexported(Unexported {
                uncommitted: 0,
                branches: vec![jackin_core::BranchName::parse("good")?],
                detached: None,
                worktrees: Vec::new(),
                unusable: vec![TerminalText::sanitize(b"jackin/evil")],
            })
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_target_ref_checked_out_in_any_host_worktree_is_refused_with_no_ref_change() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let first = handle.commit_in_project("a.txt", b"a", "one").await?;
        export(&rig, &name).await?;
        let target = ref_of(&name, "main");
        let short = target.trim_start_matches("refs/heads/").to_owned();
        let second = handle.commit_in_project("b.txt", b"b", "two").await?;
        rig.repo.git(&["checkout", "-q", &short]).await?;
        let before = snapshot(&rig).await?;

        let refused = export(&rig, &name).await;

        let Err(ExportError::RefCheckedOut { host_ref, worktree }) = refused else {
            return Err(format!("expected a refusal, saw {refused:?}").into());
        };
        assert_eq!(
            (host_ref.as_str(), &worktree),
            (target.as_str(), rig.repo.root())
        );
        let after = snapshot(&rig).await?;
        assert_eq!(after.rest, before.rest);
        assert_eq!(
            after.exported.get(&target).map(String::as_str),
            Some(first.as_str())
        );

        rig.repo.git(&["checkout", "-q", "main"]).await?;
        let other = tempfile::tempdir()?;
        let path = other.path().join("second");
        let path_text = path.to_string_lossy().into_owned();
        rig.repo
            .git(&["worktree", "add", "-q", &path_text, &short])
            .await?;
        let blocked = export(&rig, &name).await;
        let Err(ExportError::RefCheckedOut { worktree, .. }) = blocked else {
            return Err(format!("expected a refusal, saw {blocked:?}").into());
        };
        assert_eq!(worktree, HostPath::parse(&std::fs::canonicalize(&path)?)?);
        assert_eq!(
            snapshot(&rig)
                .await?
                .exported
                .get(&target)
                .map(String::as_str),
            Some(first.as_str())
        );

        rig.repo
            .git(&["worktree", "remove", "--force", &path_text])
            .await?;
        export(&rig, &name).await?;
        assert_eq!(
            snapshot(&rig)
                .await?
                .exported
                .get(&target)
                .map(String::as_str),
            Some(second.as_str())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_worktree_that_takes_a_target_ref_during_the_download_still_blocks_the_update()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let first = handle.commit_in_project("a.txt", b"a", "one").await?;
        export(&rig, &name).await?;
        let target = ref_of(&name, "main");
        let short = target.trim_start_matches("refs/heads/").to_owned();
        handle.commit_in_project("b.txt", b"b", "two").await?;
        let gateway = &rig.world.gateway;
        let held = Trigger::Op(GatewayOp::Download);
        gateway.inject(Fault::HoldAt {
            trigger: held.clone(),
        });
        let other = tempfile::tempdir()?;
        let path = other.path().join("late").to_string_lossy().into_owned();

        let exporting = export(&rig, &name);
        let racing = async {
            assert!(gateway.until_held(&held).await);
            let added = rig
                .repo
                .git(&["worktree", "add", "-q", &path, &short])
                .await;
            gateway.release(&held);
            added
        };
        let (refused, added) = tokio::join!(exporting, racing);

        added?;
        let Err(ExportError::RefCheckedOut { host_ref, .. }) = refused else {
            return Err(format!("expected a refusal, saw {refused:?}").into());
        };
        assert_eq!(host_ref, target);
        assert_eq!(
            snapshot(&rig)
                .await?
                .exported
                .get(&target)
                .map(String::as_str),
            Some(first.as_str())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_branch_name_with_a_control_or_format_character_is_unusable_and_plain_unicode_still_exports()
-> TestResult {
    guard(async {
        for name in ["feat\u{9b}2J", "ok\u{202e}gnp", "a\u{85}b", "x\u{200b}y"] {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let sandbox = handle.name();
            handle.git(&["branch", name]).await?;
            handle.git(&["check-ref-format", "--branch", name]).await?;
            let before = snapshot(&rig).await?;
            let shown = TerminalText::sanitize(name.as_bytes());

            let refused = export(&rig, &sandbox).await;

            assert!(
                matches!(&refused, Err(ExportError::UnusableBranch { branch }) if *branch == shown),
                "{name:?}: {refused:?}"
            );
            let after = snapshot(&rig).await?;
            assert!(exports(&after).is_empty(), "{name:?}");
            assert_eq!(after.rest, before.rest, "{name:?}");
            let expected = WorkState::Unexported(Unexported {
                uncommitted: 0,
                branches: Vec::new(),
                detached: None,
                worktrees: Vec::new(),
                unusable: vec![shown],
            });
            assert_eq!(state(&rig, &sandbox).await, expected, "{name:?}");
            let purge = rig
                .world
                .session
                .purge(&rig.config, &sandbox, PurgeMode::Guarded)
                .await;
            assert!(
                matches!(purge, Err(PurgeError::WorkNotExported { .. })),
                "{name:?}: {purge:?}"
            );
            assert!(rig.world.gateway.sandbox(&sandbox).is_some());
        }

        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let sandbox = handle.name();
        branch_with_commit(&handle, "caf\u{e9}", "a.txt").await?;
        branch_with_commit(&handle, "\u{65e5}\u{672c}\u{8a9e}/x", "b.txt").await?;
        export(&rig, &sandbox).await?;
        let landed = exports(&snapshot(&rig).await?);
        assert!(landed.contains_key(&ref_of(&sandbox, "caf\u{e9}")));
        assert!(landed.contains_key(&ref_of(&sandbox, "\u{65e5}\u{672c}\u{8a9e}/x")));
        Ok(())
    })
    .await
}
