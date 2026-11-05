// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{BranchName, TerminalText};
use jackin_session::{ExportError, ExportedBranch, Unexported, WorkState};
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{export, exports, ref_of, rewrite_packed, snapshot, state, tip};

const LATIN1: &[u8] = b"refs/heads/caf\xe9";

#[tokio::test]
async fn a_sandbox_branch_with_a_non_utf8_name_is_unusable_and_never_exported() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let work = handle.commit_in_project("a.txt", b"a", "work").await?;
        handle.git(&["pack-refs", "--all", "--prune"]).await?;
        let git_dir = handle.root().join("project/.git");
        rewrite_packed(&git_dir, &[(LATIN1.to_vec(), &work)], &[])?;
        let before = snapshot(&rig).await?;
        let shown = TerminalText::sanitize(b"caf\xe9");

        let refused = export(&rig, &name).await;

        assert!(
            matches!(&refused, Err(ExportError::UnusableBranch { branch }) if *branch == shown),
            "{refused:?}"
        );
        let after = snapshot(&rig).await?;
        assert!(exports(&after).is_empty());
        assert_eq!(after.rest, before.rest);
        assert_eq!(
            state(&rig, &name).await,
            WorkState::Unexported(Unexported {
                uncommitted: 0,
                branches: vec![BranchName::parse("main")?],
                detached: None,
                worktrees: Vec::new(),
                unusable: vec![shown],
            })
        );

        rewrite_packed(&git_dir, &[], &[LATIN1.to_vec()])?;
        export(&rig, &name).await?;
        assert!(exports(&snapshot(&rig).await?).contains_key(&ref_of(&name, "main")));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_host_branch_with_a_non_utf8_name_is_not_taken_for_a_sandbox_branch_with_replacement_characters()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let base = tip(&handle, "main").await?;
        rig.repo.git(&["pack-refs", "--all", "--prune"]).await?;
        rewrite_packed(
            rig.repo.root().as_path().join(".git").as_path(),
            &[(LATIN1.to_vec(), &base)],
            &[],
        )?;
        let lossy = "caf\u{fffd}";
        handle.git(&["branch", lossy]).await?;
        let expected = BranchName::parse(lossy)?;
        assert_eq!(
            state(&rig, &name).await,
            WorkState::Unexported(Unexported {
                uncommitted: 0,
                branches: vec![expected.clone()],
                detached: None,
                worktrees: Vec::new(),
                unusable: Vec::new(),
            })
        );

        let report = export(&rig, &name).await?;

        let exported: Vec<(&BranchName, &str)> = report
            .branches
            .iter()
            .map(
                |ExportedBranch {
                     branch, host_ref, ..
                 }| (branch, host_ref.as_str()),
            )
            .collect();
        assert_eq!(exported, [(&expected, ref_of(&name, lossy).as_str())]);
        let landed = rig
            .repo
            .git(&["for-each-ref", "--format=%(refname)", "refs/heads/jackin/"])
            .await?;
        assert_eq!(landed.trim(), ref_of(&name, lossy));
        Ok(())
    })
    .await
}

#[cfg(target_os = "linux")]
mod linux_only {
    use std::os::unix::ffi::OsStrExt;

    use jackin_core::TerminalText;
    use jackin_session::Unread;
    use jackin_testkit::{Rig, TestResult, guard};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn an_untracked_file_with_a_non_utf8_name_is_listed_unread() -> TestResult {
        guard(async {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let file = handle
                .root()
                .join("project")
                .join(std::ffi::OsStr::from_bytes(b"caf\xe9.txt"));
            std::fs::write(&file, b"x\n")?;

            let found = rig
                .world
                .session
                .changes(&handle.name(), &CancellationToken::new())
                .await?;

            let listed: Vec<(&str, Option<Unread>)> = found
                .files
                .iter()
                .map(|changed| (changed.path.as_str(), changed.unread))
                .collect();
            assert_eq!(
                listed,
                [(
                    TerminalText::sanitize(b"caf\xe9.txt").as_str(),
                    Some(Unread::UndecodableName)
                )]
            );
            Ok(())
        })
        .await
    }
}
