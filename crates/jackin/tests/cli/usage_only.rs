// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_testkit::{Rig, TestResult, guard, launch_id};

use super::support::{Harness, failed};

#[tokio::test]
async fn a_usage_only_default_account_is_refused_before_the_image_build() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&format!(
            "{}\n[defaults.accounts]\nzai = \"vault\"\n\n[accounts.vault]\nprovider = \"zai\"\ncredential = {{ source = \"op\", reference = \"op://vault/item/key\" }}\n",
            rig.config.render(),
        ))?;
        let ran = harness
            .run(&["load", "--launch-id", &launch_id(1).to_string()])
            .await?;
        failed(&ran, ExitClass::Failure, &["vault", "usage"])?;
        assert!(rig.world.engine.builds().is_empty());
        assert!(rig.world.gateway.sandboxes().is_empty());
        assert!(rig.world.gateway.providers().is_empty());
        Ok(())
    })
    .await
}
