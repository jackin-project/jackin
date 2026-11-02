// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::Rig;

use super::support::{TestResult, guard, ready};
use crate::trap::Trap;

#[tokio::test]
async fn a_launch_runs_no_program_named_by_the_project_repository_config() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let trap = Trap::new().await?;
        trap.arm(rig.repo.root().as_path()).await?;
        trap.prove_armed(rig.repo.root().as_path()).await?;
        ready(rig.launch_observed(1).await?)?;
        assert_eq!(trap.fired(), "");
        Ok(())
    })
    .await
}
