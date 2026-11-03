// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin::ExitClass;
use jackin_core::{AccountName, GatewayName};
use jackin_testkit::{FakeGateway, Rig, TestResult, World, guard};

use super::account_manage::on_two_gateways;
use super::support::{FixedConnector, GATEWAY_CONFIG, Harness, failed};

fn second() -> Result<GatewayName, jackin_core::IdError> {
    GatewayName::parse("second")
}

#[tokio::test]
async fn gateway_rm_forgets_the_providers_of_that_gateway_only_and_then_removes_the_entry()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let fleet = on_two_gateways(&rig, true).await?;

        fleet
            .harness
            .run(&["gateway", "rm", "second"])
            .await?
            .ok()?;

        assert!(
            fleet.second.provider(&fleet.provider).is_none(),
            "the removed gateway still holds {}",
            fleet.provider
        );
        assert!(
            rig.gateway().provider(&fleet.provider).is_some(),
            "the other gateway lost its provider"
        );
        let stored = fleet.harness.stored().await?;
        assert!(stored.gateway(&second()?).is_none());
        assert!(stored.account(&AccountName::parse("work")?).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateway_rm_fails_closed_naming_a_gateway_it_cannot_reach_and_changes_nothing() -> TestResult
{
    guard(async {
        let rig = Rig::new().await?;
        let mut fleet = on_two_gateways(&rig, true).await?;
        fleet.harness.connector =
            FixedConnector::empty().with(Arc::<FakeGateway>::clone(rig.gateway()));
        let before = fleet.harness.config_bytes()?;

        let ran = fleet.harness.run(&["gateway", "rm", "second"]).await?;

        failed(&ran, ExitClass::Failure, &["second", "unreachable"])?;
        assert_eq!(fleet.harness.config_bytes()?, before);
        assert!(fleet.second.provider(&fleet.provider).is_some());
        assert!(rig.gateway().provider(&fleet.provider).is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateway_rm_needs_no_gateway_but_the_one_it_removes() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let mut fleet = on_two_gateways(&rig, true).await?;
        fleet.harness.connector =
            FixedConnector::empty().with(Arc::<FakeGateway>::clone(&fleet.second));

        fleet
            .harness
            .run(&["gateway", "rm", "second"])
            .await?
            .ok()?;

        assert!(
            fleet.second.provider(&fleet.provider).is_none(),
            "the removed gateway still holds {}",
            fleet.provider
        );
        assert!(fleet.harness.stored().await?.gateway(&second()?).is_none());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn gateway_rm_never_connects_when_no_account_has_an_openshell_provider() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let mut harness = Harness::new(&world);
        harness.connector = FixedConnector::empty();
        harness.write_config(&format!(
            "{GATEWAY_CONFIG}
[gateways.second]
openshell_workspace = \"default\"
builder = \"b\"
platform = \"linux/arm64\"

[accounts.one]
provider = \"codex\"
credential = {{ source = \"env\", var = \"HOST_SECRET_VAR\" }}
"
        ))?;

        harness.run(&["gateway", "rm", "second"]).await?.ok()?;

        assert!(harness.stored().await?.gateway(&second()?).is_none());
        Ok(())
    })
    .await
}
