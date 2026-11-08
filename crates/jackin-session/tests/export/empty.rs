// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::sandbox_layout;
use jackin_session::{Unexported, WorkState};
use jackin_testkit::{RepoFixture, Rig, TestResult, guard};

use super::support::{export, exports, ref_of, snapshot};

#[tokio::test]
async fn a_repository_with_no_commits_exports_a_sandbox_branch_into_a_missing_bundle_directory()
-> TestResult {
    guard(async {
        let repo = RepoFixture::new().await?;
        repo.file("a.txt", b"a\n").await?;
        let rig = Rig::builder().repo(repo).build().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let tip = handle.commit_in_project("b.txt", b"b", "first").await?;
        assert!(handle.files().keys().all(|path| {
            !path
                .as_str()
                .starts_with(sandbox_layout::BUNDLE_DIR.as_str())
        }));
        let session = &rig.world.session;
        let state = session
            .work_state(
                &rig.config,
                &name,
                &tokio_util::sync::CancellationToken::new(),
            )
            .await;
        assert_eq!(
            state,
            WorkState::Unexported(Unexported {
                uncommitted: 0,
                branches: vec![jackin_core::BranchName::parse("main")?],
                detached: None,
                worktrees: Vec::new(),
                unusable: Vec::new(),
            })
        );

        let report = export(&rig, &name).await?;

        assert!(report.bundle.is_some());
        let refs = exports(&snapshot(&rig).await?);
        assert_eq!(
            refs.get(&ref_of(&name, "main")).map(String::as_str),
            Some(tip.as_str())
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
