// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin::ExitClass;
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_testkit::{Fault, Rig, TestResult, guard};

use super::support::{Failure, Harness, failed};

async fn running() -> Result<(Rig, Harness, String), Failure> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let name = rig.handle(1).await?.name().as_str().to_owned();
    let harness = Harness::new(&rig.world);
    harness.write_config(&rig.config.render())?;
    Ok((rig, harness, name))
}

fn cancelled() -> GatewayError {
    GatewayError::Cancelled {
        op: GatewayOp::ReadFiles,
        group: None,
    }
}

#[tokio::test]
async fn a_cancelled_agent_restart_exits_with_the_cancellation_status() -> TestResult {
    guard(async {
        let (rig, harness, name) = running().await?;
        rig.world.gateway.inject(Fault::FailNext {
            op: GatewayOp::ReadFiles,
            error: cancelled(),
        });
        let ran = harness
            .run(&["agent", "restart", name.as_str(), "omp"])
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stderr.contains("cancelled"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("argument"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_secondary_restart_exits_with_the_cancellation_status() -> TestResult {
    guard(async {
        let (rig, harness, name) = running().await?;
        rig.world.gateway.inject(Fault::FailAfter {
            op: GatewayOp::ReadFiles,
            calls: 1,
            error: cancelled(),
        });
        let ran = harness
            .run(&["agent", "restart", name.as_str(), "omp"])
            .await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stderr.contains("cancelled"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("argument"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancelled_integrate_exits_with_the_cancellation_status() -> TestResult {
    guard(async {
        let (rig, harness, name) = running().await?;
        rig.world.gateway.inject(Fault::FailNext {
            op: GatewayOp::ReadFiles,
            error: cancelled(),
        });
        let ran = harness.run(&["integrate", name.as_str()]).await?;
        assert_eq!(ran.exit, ExitClass::Cancelled, "{}", ran.stderr);
        assert!(ran.stderr.contains("cancelled"), "{}", ran.stderr);
        assert!(!ran.stderr.contains("argument"), "{}", ran.stderr);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_gateway_failure_in_a_secondary_restart_is_a_failure_and_not_a_usage_error() -> TestResult
{
    guard(async {
        let (rig, harness, name) = running().await?;
        rig.world.gateway.inject(Fault::FailAfter {
            op: GatewayOp::ReadFiles,
            calls: 1,
            error: GatewayError::Protocol {
                message: "the agents registry is unreadable".to_owned(),
            },
        });
        let ran = harness
            .run(&["agent", "restart", name.as_str(), "omp"])
            .await?;
        failed(&ran, ExitClass::Failure, &["agents registry is unreadable"])?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_unknown_agent_restart_stays_a_usage_error() -> TestResult {
    guard(async {
        let (_rig, harness, name) = running().await?;
        let ran = harness
            .run(&["agent", "restart", name.as_str(), "omp"])
            .await?;
        failed(&ran, ExitClass::Usage, &["omp", "not in this sandbox"])?;
        Ok(())
    })
    .await
}
