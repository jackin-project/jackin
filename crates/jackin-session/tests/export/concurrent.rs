// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{Rig, TestResult, guard};
use tokio_util::sync::CancellationToken;

use super::support::{bundles_left, exports, ref_of, snapshot, staging_left};

#[tokio::test]
async fn two_concurrent_exports_of_one_sandbox_both_succeed_and_leave_no_staging_and_no_bundle()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let tip = handle.commit_in_project("a.txt", b"a", "work").await?;
        let session = &rig.world.session;

        let (first, second) = tokio::join!(
            session.export(&rig.config, &name, CancellationToken::new()),
            session.export(&rig.config, &name, CancellationToken::new()),
        );

        assert_eq!(first?.branches, second?.branches);
        let refs = exports(&snapshot(&rig).await?);
        assert_eq!(
            refs.get(&ref_of(&name, "main")).map(String::as_str),
            Some(tip.as_str())
        );
        assert_eq!(staging_left(&rig)?, Vec::<String>::new());
        assert_eq!(bundles_left(&handle), Vec::<String>::new());
        Ok(())
    })
    .await
}
