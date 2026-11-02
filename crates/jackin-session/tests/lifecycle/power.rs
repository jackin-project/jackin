// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{Labels, Phase, SandboxName, SupervisorState, sandbox_layout};
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
    name: &SandboxName,
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
        assert!(
            (READY_LIMIT..=READY_LIMIT + Duration::from_secs(1)).contains(&after),
            "{after:?}"
        );
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

#[tokio::test]
async fn a_sandbox_with_unreadable_labels_is_refused_before_any_power_change() -> TestResult {
    guard(async {
        let rig = rig().await?;
        let session = &rig.world.session;
        let gateway = &rig.world.gateway;
        let broken = SandboxName::parse("jackin-broken-0001")?;
        let managed_only =
            Labels::from_pairs([("jackin.dev/managed".to_owned(), "true".to_owned())])?;
        gateway.insert_unmanaged(&broken, managed_only.clone(), Phase::Ready)?;
        let stop = session.power(&broken, PowerAction::Stop).await;
        assert!(
            matches!(&stop, Err(LifecycleError::Labels { sandbox, .. }) if *sandbox == broken),
            "{stop:?}"
        );
        let handle = gateway.sandbox(&broken).ok_or("no sandbox")?;
        assert_eq!(handle.phase(), Phase::Ready);

        let sleeper = SandboxName::parse("jackin-broken-0002")?;
        gateway.insert_unmanaged(&sleeper, managed_only, Phase::Stopped)?;
        let start = session.power(&sleeper, PowerAction::Start).await;
        assert!(
            matches!(&start, Err(LifecycleError::Labels { sandbox, .. }) if *sandbox == sleeper),
            "{start:?}"
        );
        let handle = gateway.sandbox(&sleeper).ok_or("no sandbox")?;
        assert_eq!(handle.phase(), Phase::Stopped);

        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
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
