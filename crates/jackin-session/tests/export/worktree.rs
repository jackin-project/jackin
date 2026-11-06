// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{HostSnapshot, RepoFixture, Rig, TestResult, guard};

use super::support::{export, ref_of};

#[tokio::test]
async fn export_from_a_linked_worktree_writes_the_shared_repository_and_no_worktree_state()
-> TestResult {
    guard(async {
        let main = RepoFixture::new().await?;
        main.file("README.md", b"readme\n").await?;
        main.commit().await?;
        let tree = main.linked_worktree("tree-branch").await?;
        let rig = Rig::builder().repo(tree).build().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.commit_in_project("work.txt", b"w", "work").await?;
        let main_before = HostSnapshot::take(main.root()).await?;
        let tree_before = HostSnapshot::take(rig.repo.root()).await?;

        let report = export(&rig, &name).await?;

        assert_eq!(report.branches.len(), 1);
        let shared = main
            .git(&["for-each-ref", "--format=%(refname)", "refs/heads/jackin/"])
            .await?;
        assert_eq!(shared.trim(), ref_of(&name, "tree-branch"));
        let main_after = HostSnapshot::take(main.root()).await?;
        let tree_after = HostSnapshot::take(rig.repo.root()).await?;
        assert_eq!(main_after.rest, main_before.rest);
        assert_eq!(tree_after.rest, tree_before.rest);
        assert!(tree_after.rest.files.contains_key(".git"));
        Ok(())
    })
    .await
}
