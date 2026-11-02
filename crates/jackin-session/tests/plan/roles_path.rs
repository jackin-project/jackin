// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::RoleFixture;

use super::roles_origin::cache;
use super::support::{TestResult, catalog, guard};
use crate::trap::Trap;

#[tokio::test]
async fn loading_a_path_role_runs_no_program_named_by_its_repository_config() -> TestResult {
    guard(async {
        let role = RoleFixture::new("sentinel")?;
        role.commit().await?;
        role.file("notes.txt", b"untracked\n").await?;
        let trap = Trap::new().await?;
        trap.arm_hostile(role.root().as_path()).await?;
        trap.prove_armed(role.root().as_path()).await?;
        let (cache, _keep) = cache()?;
        let (catalog, _runner) = catalog(&cache);
        catalog.load_path(role.root()).await?;
        assert_eq!(trap.fired(), "");
        Ok(())
    })
    .await
}
