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
        let root = rig.repo.root().as_path();
        trap.arm_hostile(root).await?;
        trap.prove_armed(root).await?;
        ready(rig.launch_observed(1).await?)?;
        assert_eq!(trap.fired(), "");
        let handle = rig.handle(1).await?;
        assert!(handle.root().join("project/src/main.rs").is_file());
        assert!(handle.root().join("project/README.md").is_file());
        Ok(())
    })
    .await
}
