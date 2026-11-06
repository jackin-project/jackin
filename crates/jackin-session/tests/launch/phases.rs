// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{AttemptId, HookKind, Phase, SupervisorState, sandbox_layout};
use jackin_openshell::{Gateway, PowerAction, Provisioning};
use jackin_session::{LaunchError, LaunchStep};
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, builder, converged, failure, guard, has_file, launch_rig, limits, ready, running,
};
use jackin_testkit::Fault;

fn preflight() -> jackin_core::SandboxPath {
    sandbox_layout::hook_path(HookKind::Preflight)
}

fn setup() -> jackin_core::SandboxPath {
    sandbox_layout::hook_path(HookKind::Setup)
}

#[tokio::test]
async fn an_ended_suspended_or_deleting_sandbox_stops_the_launch_and_stays() -> TestResult {
    guard(async {
        let rows = [
            (0_u8, Phase::Completed { exit_code: 0 }),
            (1, Phase::Error { exit_code: Some(3) }),
            (2, Phase::Suspended),
            (3, Phase::Deleting),
        ];
        for (row, phase) in rows {
            let rig = launch_rig().await?;
            ready(rig.launch_observed(1).await?)?;
            let handle = rig.handle(1).await?;
            match row {
                0 => handle.exit_main(0),
                1 => handle.exit_main(3),
                _ => rig.gateway().force_phase(&handle.name(), phase),
            }
            let before = (handle.files(), format!("{:?}", handle.labels()));
            let failed = failure(rig.launch_observed(1).await?)?;
            assert_eq!(failed.step, LaunchStep::Ready);
            assert_eq!(failed.sandbox, Some(handle.name()));
            match (&*failed.error, row) {
                (LaunchError::MainProcessEnded { phase: found, .. }, 0 | 1 | 3) => {
                    assert_eq!(*found, phase);
                }
                (LaunchError::SandboxSuspended { sandbox }, 2) => {
                    assert_eq!(*sandbox, handle.name());
                }
                (other, row) => return Err(format!("row {row}: {other:?}").into()),
            }
            assert_eq!(rig.gateway().sandboxes().len(), 1);
            assert_eq!(handle.phase(), phase);
            assert_eq!((handle.files(), format!("{:?}", handle.labels())), before);
            assert_eq!(handle.runs(&setup()).len(), 1);
            assert_eq!(handle.runs(&preflight()).len(), 1);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_settling_sandbox_is_waited_for_on_the_clock_until_ready() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::ProvisionFor { polls: 5 });
        ready(rig.launch_observed(1).await?)?;
        let slept: Duration = rig.world.clock.slept().iter().sum();
        assert!(!rig.world.clock.slept().is_empty());
        assert!(slept < Duration::from_secs(30), "{slept:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_that_settles_too_slowly_times_out_on_the_clock_and_a_patient_launch_joins_it()
-> TestResult {
    guard(async {
        let rig = builder().await?.limits(limits(5, 20)).build().await?;
        rig.gateway().inject(Fault::ProvisionFor { polls: 40 });
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Ready);
        assert!(
            matches!(&*failed.error, LaunchError::ReadyTimeout { phase: Phase::Provisioning, after, .. } if *after >= Duration::from_secs(5)),
            "{:?}",
            failed.error
        );
        assert!(rig.world.clock.slept().iter().sum::<Duration>() >= Duration::from_secs(5));
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let patient = rig.session_with(limits(600, 20));
        let outcome = rig.run_on(&patient, rig.request(1)?, CancellationToken::new()).await?;
        ready(outcome)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_in_an_unknown_phase_times_out_on_the_clock_and_stays() -> TestResult {
    guard(async {
        let rig = builder().await?.limits(limits(5, 20)).build().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        rig.gateway().force_phase(&handle.name(), Phase::Unknown);
        let failed = failure(rig.launch_observed(1).await?)?;
        assert!(
            matches!(
                &*failed.error,
                LaunchError::ReadyTimeout {
                    phase: Phase::Unknown,
                    ..
                }
            ),
            "{:?}",
            failed.error
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_stopped_sandbox_is_started_and_restarts_at_attempt_one() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        rig.gateway()
            .set_power(&handle.name(), PowerAction::Stop, &CancellationToken::new())
            .await?;
        rig.repo.file("late.txt", b"late\n").await?;
        let target = ready(rig.launch_observed(1).await?)?;
        assert_eq!(target.state, running(1)?);
        assert_eq!(handle.phase(), Phase::Ready);
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_eq!(handle.runs(&setup()).len(), 1);
        assert_eq!(handle.runs(&preflight()).len(), 2);
        assert!(!has_file(
            &handle,
            &super::support::path("/sandbox/project/late.txt")?
        ));
        converged(&rig, &handle, 1)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_stale_running_state_after_power_start_is_not_accepted() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        rig.gateway()
            .set_power(&handle.name(), PowerAction::Stop, &CancellationToken::new())
            .await?;
        rig.gateway()
            .inject(Fault::StaleSupervisor { observations: 3 });
        let target = ready(rig.launch_observed(1).await?)?;
        assert_eq!(target.state, running(1)?);
        assert_eq!(handle.runs(&preflight()).len(), 2);
        assert_eq!(handle.supervisor(), Some(running(1)?));
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_transient_provisioning_failure_is_deleted_and_recreated_up_to_create_attempts()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::ProvisionFails { creates: 2 });
        let target = ready(rig.launch_observed(1).await?)?;
        assert_eq!(target.sandbox, rig.plan(1)?.sandbox_name().clone());
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        assert_eq!(rig.handle(1).await?.phase(), Phase::Ready);

        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::ProvisionFails { creates: 5 });
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        let name = rig.plan(1)?.sandbox_name().clone();
        assert!(
            matches!(&*failed.error, LaunchError::ProvisioningFailed { sandbox, attempts: 3, message } if *sandbox == name && !message.as_str().is_empty()),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        assert_eq!(handle.phase(), Phase::Error { exit_code: None });
        let again = failure(rig.launch_observed(1).await?)?;
        assert!(
            matches!(&*again.error, LaunchError::MainProcessEnded { phase: Phase::Error { exit_code: None }, .. }),
            "{:?}",
            again.error
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        assert_eq!(handle.phase(), Phase::Error { exit_code: None });
        let view = rig
            .gateway()
            .find_sandboxes(&jackin_openshell::SandboxQuery::Name(name), &CancellationToken::new())
            .await?;
        assert!(matches!(view[0].provisioning, Provisioning::Failed { .. }));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_start_the_gateway_ignores_is_waited_out_on_the_clock_to_a_ready_timeout() -> TestResult {
    guard(async {
        let rig = builder().await?.limits(limits(5, 20)).build().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        rig.gateway()
            .set_power(&handle.name(), PowerAction::Stop, &CancellationToken::new())
            .await?;
        rig.gateway().inject(Fault::StartIgnored);
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Ready);
        assert!(
            matches!(&*failed.error, LaunchError::ReadyTimeout { phase: Phase::Stopped, after, .. } if *after >= Duration::from_secs(5)),
            "{:?}",
            failed.error
        );
        assert!(rig.world.clock.slept().iter().sum::<Duration>() >= Duration::from_secs(5));
        assert_eq!(handle.phase(), Phase::Stopped);
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_stale_exited_state_after_power_start_is_waited_out_and_attempt_one_starts() -> TestResult
{
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        handle.exit_codex(3);
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Exited {
                attempt: AttemptId::FIRST,
                code: 3
            })
        );
        rig.gateway()
            .set_power(&handle.name(), PowerAction::Stop, &CancellationToken::new())
            .await?;
        rig.gateway()
            .inject(Fault::StaleSupervisor { observations: 3 });
        let target = ready(rig.launch_observed(1).await?)?;
        assert_eq!(target.state, running(1)?);
        assert_eq!(handle.supervisor(), Some(running(1)?));
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_eq!(handle.runs(&preflight()).len(), 2);
        Ok(())
    })
    .await
}
