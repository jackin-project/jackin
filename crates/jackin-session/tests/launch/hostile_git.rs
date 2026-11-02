// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::Rig;
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard};
use crate::trap::Trap;

#[tokio::test]
async fn an_export_runs_no_program_named_by_the_project_repository_config() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let trap = Trap::new().await?;
        trap.arm(rig.repo.root().as_path()).await?;
        trap.prove_armed(rig.repo.root().as_path()).await?;
        handle.git(&["checkout", "-q", "-b", "work"]).await?;
        let tip = handle
            .commit_in_project("work.txt", b"work\n", "work")
            .await?;
        rig.world
            .session
            .export(&rig.config, &handle.name(), CancellationToken::new())
            .await?;
        assert_eq!(trap.fired(), "");
        let exported = format!("refs/heads/jackin/{}/work", handle.name());
        assert_eq!(
            rig.repo.git(&["rev-parse", &exported]).await?.trim(),
            tip.as_str()
        );
        Ok(())
    })
    .await
}
