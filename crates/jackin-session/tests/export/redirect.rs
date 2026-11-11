// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_session::{ExportError, UnsupportedProject};
use jackin_testkit::{HostSnapshot, RepoFixture, Rig, TestResult, guard};

use super::support::{branch_with_commit, export, staging_left};

#[tokio::test]
async fn an_export_from_a_workspace_whose_gitfile_names_another_repository_writes_no_ref_there()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        branch_with_commit(&handle, "work", "w.txt").await?;
        let victim = RepoFixture::new().await?;
        victim.file("private.txt", b"TOP-SECRET-TOKEN\n").await?;
        victim.commit().await?;
        let before = HostSnapshot::take(victim.root()).await?;
        let root = rig.repo.root().clone();
        std::fs::remove_dir_all(root.as_path().join(".git"))?;
        std::fs::write(
            root.as_path().join(".git"),
            format!(
                "gitdir: {}\n",
                victim.root().as_path().join(".git").display()
            ),
        )?;

        match export(&rig, &handle.name()).await {
            Err(ExportError::Unsupported(UnsupportedProject::ForeignGit(named))) => {
                assert_eq!(named, root);
            }
            other => return Err(format!("expected ForeignGit, saw {other:?}").into()),
        }

        let after = HostSnapshot::take(victim.root()).await?;
        assert_eq!(after.exported, BTreeMap::new());
        assert_eq!(after.rest, before.rest);
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        Ok(())
    })
    .await
}
