// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_openshell::{GatewayError, GatewayOp, Missing};
use jackin_session::{LaunchError, LaunchStep};
use jackin_testkit::{Fault, Rig};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, failure, guard, launch_rig, ready};

fn exists(rig: &Rig) -> Result<GatewayError, Box<dyn std::error::Error>> {
    Ok(GatewayError::AlreadyExists {
        what: Missing::Sandbox(rig.sandbox_name(1)?),
    })
}

fn slept(rig: &Rig) -> Duration {
    rig.world.clock.slept().iter().sum()
}

#[tokio::test]
async fn a_create_that_reports_a_name_clash_with_no_sandbox_pauses_before_it_retries() -> TestResult
{
    guard(async {
        let control = launch_rig().await?;
        ready(control.launch_observed(1).await?)?;

        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::FailNext {
            op: GatewayOp::Create,
            error: exists(&rig)?,
        });
        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        assert!(
            slept(&rig) > slept(&control),
            "{:?}",
            rig.world.clock.slept()
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn repeated_name_clashes_with_no_sandbox_stop_at_the_create_attempt_limit() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        for _ in 0..5 {
            rig.gateway().inject(Fault::FailNext {
                op: GatewayOp::Create,
                error: exists(&rig)?,
            });
        }
        let failed = failure(rig.run(rig.request(1)?, CancellationToken::new()).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert!(
            matches!(
                *failed.error,
                LaunchError::NameCollision { other: None, .. }
            ),
            "{:?}",
            failed.error
        );
        assert!(rig.gateway().sandboxes().is_empty());
        Ok(())
    })
    .await
}
