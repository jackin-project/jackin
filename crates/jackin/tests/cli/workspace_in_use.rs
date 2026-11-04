// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_core::WorkspaceName;
use jackin_testkit::{Rig, TestResult, guard};

use super::support::{FixedConnector, Harness, failed};

const WORKSPACE: &str = "platform-backend";

#[tokio::test]
async fn workspace_rm_is_refused_while_a_sandbox_carries_its_label_and_works_after_the_purge()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let name = rig.sandbox_name(1)?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.write_config(&rig.config.render())?;
        let before = harness.config_bytes()?;

        let refused = harness.run(&["workspace", "rm", WORKSPACE]).await?;
        failed(
            &refused,
            ExitClass::Failure,
            &[name.as_str(), WORKSPACE, "export", "purge"],
        )?;
        assert_eq!(harness.config_bytes()?, before);

        harness
            .run(&["purge", name.as_str(), "--force"])
            .await?
            .ok()?;
        harness.run(&["workspace", "rm", WORKSPACE]).await?.ok()?;
        assert!(
            harness
                .stored()
                .await?
                .workspace(&WorkspaceName::parse(WORKSPACE)?)
                .is_none()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn workspace_rm_is_refused_when_a_gateway_cannot_be_asked_about_its_sandboxes() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        let mut harness = Harness::new(&rig.world);
        harness.cwd = rig.repo.root().clone();
        harness.connector = FixedConnector::empty();
        harness.write_config(&rig.config.render())?;
        let before = harness.config_bytes()?;

        let refused = harness.run(&["workspace", "rm", WORKSPACE]).await?;
        failed(&refused, ExitClass::Failure, &[])?;
        assert_eq!(harness.config_bytes()?, before);
        Ok(())
    })
    .await
}
