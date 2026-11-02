// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::{ConnectError, ExitClass, GatewayConnector};
use jackin_core::GatewayName;
use jackin_testkit::{ProcessReply, TestResult, World, guard};
use tokio_util::sync::CancellationToken;

use super::support::{connecting, failed};

#[tokio::test]
async fn a_missing_openshell_is_a_discovery_error_and_not_an_unreachable_gateway() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (harness, connector) = connecting(&world, ProcessReply::spawn("openshell-is-absent"))?;
        let config = harness.stored().await?;
        let name = GatewayName::parse("fake-gateway")?;
        let entry = config.gateway(&name).ok_or("no gateway")?;
        match connector
            .connect(&name, entry, &CancellationToken::new())
            .await
        {
            Err(ConnectError::Discovery { gateway, .. }) => assert_eq!(gateway, name),
            Err(other) => return Err(format!("not a discovery error: {other}").into()),
            Ok(_) => return Err("connected without openshell".into()),
        }
        let ran = harness.run(&["ls"]).await?;
        failed(
            &ran,
            ExitClass::Failure,
            &["fake-gateway", "openshell-is-absent"],
        )?;
        assert!(!ran.stderr.contains("unreachable"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_during_discovery_exits_cancelled_at_once() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (harness, _) = connecting(&world, ProcessReply::until_cancelled(Vec::new()))?;
        let token = CancellationToken::new();
        token.cancel();
        let ran = harness.command(&["ls"]).cancel(token).run().await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stdout.is_empty());
        Ok(())
    })
    .await
}
