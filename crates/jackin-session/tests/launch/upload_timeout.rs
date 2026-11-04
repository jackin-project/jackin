// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::time::Duration;

use jackin_core::{HostPath, sandbox_layout};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{LaunchError, LaunchLimits, LaunchStep};
use jackin_testkit::{Fault, Rig};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, builder, launch_id, limits, ready};

fn upload_limit(seconds: u64) -> LaunchLimits {
    LaunchLimits {
        upload: Duration::from_secs(seconds),
        ..limits(30, 20)
    }
}

async fn with_limit(seconds: u64) -> Result<Rig, Box<dyn std::error::Error>> {
    builder().await?.limits(upload_limit(seconds)).build().await
}

fn ready_outcome(
    result: Result<jackin_session::LaunchOutcome, jackin_session::LaunchFailure>,
) -> TestResult {
    result
        .map(drop)
        .map_err(|failure| format!("{failure:?}").into())
}

#[tokio::test]
async fn an_upload_that_exceeds_its_limit_fails_with_a_typed_timeout_and_a_retry_converges()
-> TestResult {
    jackin_testkit::guard_within(Duration::from_secs(20), async {
        let rig = with_limit(1).await?;
        rig.world.gateway.inject(Fault::StallProgram {
            program: sandbox_layout::GIT.clone(),
        });
        let failed = rig
            .launch_with(1, CancellationToken::new())
            .await?
            .err()
            .ok_or("the stalled upload finished")?;
        assert_eq!(failed.step, LaunchStep::Upload);
        assert!(
            matches!(
                *failed.error,
                LaunchError::Gateway(GatewayError::Timeout {
                    op: GatewayOp::UploadTree,
                    after, .. }) if after == Duration::from_secs(1)
            ),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        assert!(
            handle
                .file(&sandbox_layout::uploaded_marker(&launch_id(1)))
                .is_none()
        );
        let staging = std::fs::read_dir(rig.world.cache_dir().as_path())?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .count();
        assert_eq!(staging, 0);

        rig.world.gateway.release_stall(&sandbox_layout::GIT);
        let roomy = rig.session_with(limits(30, 20));
        ready_outcome(
            rig.run_on(&roomy, rig.request(1)?, CancellationToken::new())
                .await?
                .result,
        )?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn time_spent_waiting_for_the_upload_lock_is_not_charged_to_the_upload_limit() -> TestResult {
    jackin_testkit::guard_within(Duration::from_secs(30), async {
        let rig = with_limit(3).await?;
        rig.world.fs.create_dir_all(rig.world.cache_dir()).await?;
        let lock = HostPath::resolve(
            Path::new(&format!("upload-{}.lock", launch_id(1))),
            rig.world.cache_dir(),
        )?;
        let held = rig.world.fs.lock(&lock).await?;
        let peer = async {
            while !rig
                .world
                .clock
                .slept()
                .contains(&Duration::from_millis(100))
            {
                tokio::task::yield_now().await;
            }
            tokio::time::sleep(Duration::from_secs(4)).await;
            drop(held);
        };
        let (outcome, ()) = tokio::join!(rig.launch_observed(1), peer);
        ready(outcome?)?;
        Ok(())
    })
    .await
}
