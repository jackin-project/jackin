// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Phase, SupervisorState, sandbox_layout};
use jackin_openshell::PowerAction;
use jackin_session::LifecycleError;
use jackin_testkit::{Fault, Rig, TestResult, default_limits, guard, launch_id};

const READY_LIMIT: Duration = Duration::from_secs(3);

async fn rig() -> Result<Rig, Box<dyn std::error::Error>> {
    let limits = jackin_session::LaunchLimits {
        ready: READY_LIMIT,
        ..default_limits()
    };
    Rig::builder().limits(limits).build().await
}

fn assert_ended(
    refused: Result<jackin_core::SandboxSummary, LifecycleError>,
    name: &jackin_core::SandboxName,
    ended: Phase,
) -> TestResult {
    match refused {
        Err(LifecycleError::MainProcessEnded { sandbox, phase }) => {
            assert_eq!((&sandbox, phase), (name, ended));
            Ok(())
        }
        other => Err(format!("{ended:?}: saw {other:?}").into()),
    }
}

#[tokio::test]
async fn stop_and_start_keep_the_files_and_reset_the_supervisor_and_a_stuck_start_times_out()
-> TestResult {
    guard(async {
        let rig = rig().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        let env_file = sandbox_layout::env_file(&launch_id(1));
        let project_file = jackin_core::SandboxPath::parse("/sandbox/project/README.md")?;
        let files = handle.files();
        assert!(files.contains_key(&env_file) && files.contains_key(&project_file));

        let stopped = session.power(&name, PowerAction::Stop).await?;
        assert_eq!(stopped.phase, Phase::Stopped);
        assert_eq!(handle.phase(), Phase::Stopped);
        assert_eq!(handle.file(&env_file), files.get(&env_file).cloned());
        assert_eq!(
            handle.file(&project_file),
            files.get(&project_file).cloned()
        );

        let again = session.power(&name, PowerAction::Stop).await?;
        assert_eq!(again.phase, Phase::Stopped);

        let started = session.power(&name, PowerAction::Start).await?;
        assert_eq!(started.phase, Phase::Ready);
        assert_eq!(handle.supervisor(), Some(SupervisorState::Waiting));
        assert_eq!(
            handle.file(&project_file),
            files.get(&project_file).cloned()
        );

        session.power(&name, PowerAction::Stop).await?;
        rig.world.gateway.inject(Fault::StartIgnored);
        let slept_before: Duration = rig.world.clock.slept().iter().sum();
        let stuck = session.power(&name, PowerAction::Start).await;
        let Err(LifecycleError::PowerTimeout {
            sandbox,
            wanted,
            after,
        }) = stuck
        else {
            return Err(format!("expected a timeout, saw {stuck:?}").into());
        };
        assert_eq!((sandbox, wanted), (name.clone(), Phase::Ready));
        assert!(after >= READY_LIMIT, "{after:?}");
        let slept: Duration = rig
            .world
            .clock
            .slept()
            .iter()
            .sum::<Duration>()
            .saturating_sub(slept_before);
        assert!(slept >= READY_LIMIT, "{slept:?}");
        assert_eq!(handle.phase(), Phase::Stopped);

        let absent = rig.sandbox_name(9)?;
        let missing = session.power(&absent, PowerAction::Stop).await;
        assert!(matches!(missing, Err(LifecycleError::NotFound(found)) if found == absent));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_sandbox_whose_main_process_ended_refuses_stop_and_start_and_stays_as_it_was()
-> TestResult {
    guard(async {
        let rig = rig().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        for ended in [
            Phase::Completed { exit_code: 0 },
            Phase::Error { exit_code: Some(3) },
            Phase::Error { exit_code: None },
        ] {
            for action in [PowerAction::Stop, PowerAction::Start] {
                rig.world.gateway.force_phase(&name, ended);
                let refused = session.power(&name, action).await;
                assert_ended(refused, &name, ended)?;
                assert_eq!(handle.phase(), ended);
            }
        }
        rig.world.gateway.force_phase(&name, Phase::Ready);
        assert_eq!(
            session.power(&name, PowerAction::Stop).await?.phase,
            Phase::Stopped
        );
        assert_eq!(
            session.power(&name, PowerAction::Start).await?.phase,
            Phase::Ready
        );
        Ok(())
    })
    .await
}
