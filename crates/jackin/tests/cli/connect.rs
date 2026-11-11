// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin::ExitClass;
use jackin_core::{GatewayName, GroupFault};
use jackin_session::{ConnectError, GatewayConnector};
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

#[tokio::test]
async fn an_unregistered_gateway_is_named_as_a_plain_word() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (harness, _) = connecting(&world, ProcessReply::exited(0, b"[]", b""))?;
        let ran = harness.run(&["ls"]).await?;
        failed(&ran, ExitClass::Failure, &[])?;
        assert_eq!(
            ran.stderr,
            "error: cannot discover gateway fake-gateway: gateway fake-gateway is not registered\n"
        );
        Ok(())
    })
    .await
}

fn still_alive() -> GroupFault {
    GroupFault::Alive {
        after: Duration::from_secs(5),
    }
}

#[tokio::test]
async fn a_cancel_whose_group_stop_failed_is_still_a_cancel_and_names_the_fault() -> TestResult {
    guard(async {
        let world = World::builder().build()?;
        let (harness, connector) = connecting(&world, ProcessReply::cancelled_with(still_alive()))?;
        let config = harness.stored().await?;
        let name = GatewayName::parse("fake-gateway")?;
        let entry = config.gateway(&name).ok_or("no gateway")?;
        match connector
            .connect(&name, entry, &CancellationToken::new())
            .await
        {
            Err(error) => assert!(error.is_cancelled(), "{error}"),
            Ok(_) => return Err("connected without openshell".into()),
        }
        let ran = harness.run(&["ls"]).await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(
            ran.stderr.contains("still alive after 5s"),
            "the fault is lost: {}",
            ran.stderr
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_timeout_or_a_group_fault_in_discovery_is_a_failure_and_never_a_cancel() -> TestResult {
    guard(async {
        for (label, reply) in [
            ("timeout", ProcessReply::timeout_with(still_alive())),
            ("group fault", ProcessReply::group(still_alive())),
        ] {
            let world = World::builder().build()?;
            let (harness, connector) = connecting(&world, reply)?;
            let config = harness.stored().await?;
            let name = GatewayName::parse("fake-gateway")?;
            let entry = config.gateway(&name).ok_or("no gateway")?;
            match connector
                .connect(&name, entry, &CancellationToken::new())
                .await
            {
                Err(error) => assert!(!error.is_cancelled(), "{label}: {error}"),
                Ok(_) => return Err(format!("{label}: connected").into()),
            }
            let ran = harness.run(&["ls"]).await?;
            assert_eq!(ran.exit, ExitClass::Failure, "{label}: {}", ran.stderr);
        }
        Ok(())
    })
    .await
}
