// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_testkit::{Rig, TestResult, guard};

use super::support::{branch_with_commit, export, exports, ref_of, snapshot};

#[tokio::test]
async fn three_export_rounds_keep_a_flat_ref_set_with_only_changed_branches() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.repo.git(&["branch", "side"]).await?;
        let mut expected = BTreeSet::new();
        let mut first = None;
        for round in 1..=3_u8 {
            rig.launch(round).await?;
            let handle = rig.handle(round).await?;
            let name = handle.name();
            let inherited = handle
                .git(&["for-each-ref", "--format=%(refname)", "refs/heads/jackin/"])
                .await?;
            assert_eq!(inherited.trim(), "");
            let branch = format!("work-{round}");
            branch_with_commit(&handle, &branch, "w.txt").await?;
            handle.git(&["tag", &format!("tag-{round}")]).await?;
            handle.git(&["checkout", "-q", "main"]).await?;
            export(&rig, &name).await?;
            expected.insert(ref_of(&name, &branch));
            if round == 1 {
                first = Some(name);
            }
            if round == 3
                && let Some(early) = &first
            {
                export(&rig, early).await?;
            }
            let found = snapshot(&rig).await?;
            let refs: BTreeSet<String> = exports(&found).into_keys().collect();
            assert_eq!(refs, expected, "round {round}");
            assert!(
                refs.iter()
                    .all(|name| !name.contains("/jackin/") || name.matches("jackin/").count() == 1)
            );
            assert!(
                found
                    .rest
                    .refs
                    .keys()
                    .all(|name| !name.starts_with("refs/tags/"))
            );
        }
        Ok(())
    })
    .await
}
