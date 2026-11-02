// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{Rig, TestResult, guard};

use super::support::{branch_with_commit, export, ref_of, snapshot};

#[tokio::test]
async fn a_symbolic_ref_at_the_target_is_replaced_and_never_written_through() -> TestResult {
    guard(async {
        for (label, destination) in [("branch", "refs/heads/main"), ("tag", "refs/tags/v1")] {
            let rig = Rig::new().await?;
            rig.launch(1).await?;
            let handle = rig.handle(1).await?;
            let name = handle.name();
            let tip = branch_with_commit(&handle, "work", "w.txt").await?;
            rig.repo.git(&["tag", "v1"]).await?;
            let target = ref_of(&name, "work");
            rig.repo
                .git(&["symbolic-ref", &target, destination])
                .await?;
            let main = rig.repo.git(&["rev-parse", "refs/heads/main"]).await?;
            let tag = rig.repo.git(&["rev-parse", "refs/tags/v1"]).await?;
            let before = snapshot(&rig).await?;

            export(&rig, &name)
                .await
                .map_err(|error| format!("{label}: {error:?}"))?;

            assert_eq!(
                rig.repo.git(&["rev-parse", &target]).await?.trim(),
                tip.as_str()
            );
            assert!(
                rig.repo
                    .git(&["symbolic-ref", "-q", &target])
                    .await
                    .is_err(),
                "{label}: the target is still a symbolic ref"
            );
            assert_eq!(
                rig.repo.git(&["rev-parse", "refs/heads/main"]).await?,
                main,
                "{label}"
            );
            assert_eq!(
                rig.repo.git(&["rev-parse", "refs/tags/v1"]).await?,
                tag,
                "{label}"
            );
            let after = snapshot(&rig).await?;
            assert_eq!(after.rest.head, before.rest.head, "{label}");
            assert_eq!(after.rest.files, before.rest.files, "{label}");
            assert_eq!(after.rest.refs, before.rest.refs, "{label}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn every_target_of_a_multi_branch_export_is_replaced_when_all_of_them_are_symbolic_refs()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let first = branch_with_commit(&handle, "a", "a.txt").await?;
        let second = branch_with_commit(&handle, "b", "b.txt").await?;
        let main = rig.repo.git(&["rev-parse", "refs/heads/main"]).await?;
        for branch in ["a", "b"] {
            rig.repo
                .git(&["symbolic-ref", &ref_of(&name, branch), "refs/heads/main"])
                .await?;
        }

        export(&rig, &name)
            .await
            .map_err(|error| format!("{error:?}"))?;

        for (branch, tip) in [("a", first), ("b", second)] {
            let target = ref_of(&name, branch);
            assert_eq!(
                rig.repo.git(&["rev-parse", &target]).await?.trim(),
                tip.as_str(),
                "{branch}"
            );
            assert!(
                rig.repo
                    .git(&["symbolic-ref", "-q", &target])
                    .await
                    .is_err(),
                "{branch}: the target is still a symbolic ref"
            );
        }
        assert_eq!(
            rig.repo.git(&["rev-parse", "refs/heads/main"]).await?,
            main,
            "main moved"
        );
        Ok(())
    })
    .await
}
