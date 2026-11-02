// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{HostSnapshot, Rig};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard};
use crate::trap::Trap;

#[tokio::test]
async fn a_launch_and_an_export_run_no_program_named_by_the_project_repository_config() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        let trap = Trap::new().await?;
        let root = rig.repo.root().as_path();
        trap.arm_hostile(root).await?;
        trap.prove_armed(root).await?;
        rig.launch(1).await?;
        assert_eq!(trap.fired(), "", "the launch ran a program of the project");
        let handle = rig.handle(1).await?;
        handle.git(&["checkout", "-q", "-b", "work"]).await?;
        let tip = handle
            .commit_in_project("work.txt", b"work\n", "work")
            .await?;
        let before = HostSnapshot::take(rig.repo.root()).await?;
        trap.clear()?;

        rig.world
            .session
            .export(&rig.config, &handle.name(), CancellationToken::new())
            .await?;

        assert_eq!(trap.fired(), "", "the export ran a program of the project");
        trap.clear()?;
        let exported = format!("refs/heads/jackin/{}/work", handle.name());
        assert_eq!(
            rig.repo.git(&["rev-parse", &exported]).await?.trim(),
            tip.as_str()
        );
        let after = HostSnapshot::take(rig.repo.root()).await?;
        assert_eq!(after.rest, before.rest);
        Ok(())
    })
    .await
}
