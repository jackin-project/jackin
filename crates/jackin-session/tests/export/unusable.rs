// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::HostPath;
use jackin_session::{ExportError, Unexported, WorkState};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{branch_with_commit, export, exports, ref_of, snapshot};

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
                Err(ExportError::UnusableBranch { branch }) if branch == "jackin/evil"
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
                uncommitted: false,
                branches: vec![jackin_core::BranchName::parse("good")?],
                detached: None,
                unusable: vec!["jackin/evil".to_owned()],
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
