// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::Sha256;
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{
    branch_with_commit, bundles_left, export, exports, ref_of, snapshot, staging_left, tip,
};

async fn rewrite_tracked(rig: &Rig) -> TestResult {
    let listed = rig.repo.git(&["ls-files"]).await?;
    for file in listed.lines() {
        let bytes = rig.repo.read(file).await?;
        rig.repo.file(file, &bytes).await?;
    }
    Ok(())
}

#[tokio::test]
async fn export_writes_only_the_changed_branches_under_the_sandbox_prefix_and_never_the_host_state()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.repo.git(&["branch", "side"]).await?;
        rig.repo.file("README.md", b"staged\n").await?;
        rig.repo.git(&["add", "README.md"]).await?;
        rig.repo.file("src/main.rs", b"fn main() { 1 }\n").await?;
        rig.repo.file("untracked.txt", b"u\n").await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let main_tip = handle.commit_in_project("a.txt", b"a", "main work").await?;
        let feature_tip = branch_with_commit(&handle, "feature/x", "x.txt").await?;
        rewrite_tracked(&rig).await?;
        let before = snapshot(&rig).await?;

        let report = export(&rig, &name).await?;

        let after = snapshot(&rig).await?;
        assert_eq!(after.rest, before.rest);
        let expected = BTreeMap::from([
            (ref_of(&name, "feature/x"), feature_tip.as_str().to_owned()),
            (ref_of(&name, "main"), main_tip.as_str().to_owned()),
        ]);
        assert_eq!(exports(&after), expected);
        assert_eq!(report.sandbox, name);
        let oracle = handle.root().join("oracle.bundle");
        handle
            .git(&[
                "bundle",
                "create",
                "--quiet",
                &oracle.display().to_string(),
                "refs/heads/feature/x",
                "refs/heads/main",
            ])
            .await?;
        assert_eq!(
            report.bundle,
            Some(Sha256::of(&tokio::fs::read(&oracle).await?))
        );
        assert_eq!(bundles_left(&handle), Vec::<String>::new());
        let listed: Vec<(String, String, String)> = report
            .branches
            .iter()
            .map(|item| {
                (
                    item.branch.as_str().to_owned(),
                    item.tip.as_str().to_owned(),
                    item.host_ref.clone(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            vec![
                (
                    "feature/x".to_owned(),
                    feature_tip.as_str().to_owned(),
                    ref_of(&name, "feature/x")
                ),
                (
                    "main".to_owned(),
                    main_tip.as_str().to_owned(),
                    ref_of(&name, "main")
                ),
            ]
        );
        assert!(report.detached.is_none());
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());

        handle
            .git(&[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@e.invalid",
                "commit",
                "-q",
                "--amend",
                "-m",
                "amended",
            ])
            .await?;
        let amended = tip(&handle, "feature/x").await?;
        assert_ne!(amended, feature_tip);
        let moved = export(&rig, &name).await?;
        let again = export(&rig, &name).await?;
        assert_eq!(again.branches, moved.branches);
        let refs = exports(&snapshot(&rig).await?);
        assert_eq!(
            refs.get(&ref_of(&name, "feature/x")).map(String::as_str),
            Some(amended.as_str())
        );
        assert_eq!(refs.len(), 2);
        Ok(())
    })
    .await
}
