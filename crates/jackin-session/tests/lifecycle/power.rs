// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::task::Poll;
use std::time::Duration;

use jackin_core::{Labels, Phase, SandboxName, SandboxSummary, SupervisorState, sandbox_layout};
use jackin_openshell::{Gateway, GatewayOp, PowerAction};
use jackin_session::{LifecycleError, ManagedIdentityError, SandboxObservation};
use jackin_testkit::{Fault, Rig, TestResult, default_limits, guard, launch_id};
use tokio_util::sync::CancellationToken;

const READY_LIMIT: Duration = Duration::from_secs(3);

async fn rig() -> Result<Rig, Box<dyn std::error::Error>> {
    let limits = jackin_session::LaunchLimits {
        ready: READY_LIMIT,
        ..default_limits()
    };
    Rig::builder().limits(limits).build().await
}

fn assert_ended(
    refused: Result<SandboxObservation, LifecycleError>,
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

fn readable(observation: SandboxObservation) -> Result<SandboxSummary, Box<dyn std::error::Error>> {
    match observation {
        SandboxObservation::Readable(summary) => Ok(summary),
        SandboxObservation::Unreadable(sandbox) => {
            Err(format!("expected readable sandbox: {}", sandbox.reason()).into())
        }
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
        let identity = session
            .managed_identity(&name, &CancellationToken::new())
            .await?;
        let env_file = sandbox_layout::env_file(&launch_id(1));
        let project_file = jackin_core::SandboxPath::parse("/sandbox/project/README.md")?;
        let files = handle.files();
        assert!(files.contains_key(&env_file) && files.contains_key(&project_file));

        let stopped = readable(
            session
                .power(&identity, PowerAction::Stop, &CancellationToken::new())
                .await?,
        )?;
        assert_eq!(stopped.phase, Phase::Stopped);
        assert_eq!(handle.phase(), Phase::Stopped);
        assert_eq!(handle.file(&env_file), files.get(&env_file).cloned());
        assert_eq!(
            handle.file(&project_file),
            files.get(&project_file).cloned()
        );

        let again = readable(
            session
                .power(&identity, PowerAction::Stop, &CancellationToken::new())
                .await?,
        )?;
        assert_eq!(again.phase, Phase::Stopped);

        let started = readable(
            session
                .power(&identity, PowerAction::Start, &CancellationToken::new())
                .await?,
        )?;
        assert_eq!(started.phase, Phase::Ready);
        assert_eq!(handle.supervisor(), Some(SupervisorState::Waiting));
        assert_eq!(
            handle.file(&project_file),
            files.get(&project_file).cloned()
        );

        session
            .power(&identity, PowerAction::Stop, &CancellationToken::new())
            .await?;
        rig.world.gateway.inject(Fault::StartIgnored);
        let slept_before: Duration = rig.world.clock.slept().iter().sum();
        let stuck = session
            .power(&identity, PowerAction::Start, &CancellationToken::new())
            .await;
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
        let missing = session
            .managed_identity(&absent, &CancellationToken::new())
            .await;
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
        let identity = session
            .managed_identity(&name, &CancellationToken::new())
            .await?;
        for ended in [
            Phase::Completed { exit_code: 0 },
            Phase::Error { exit_code: Some(3) },
            Phase::Error { exit_code: None },
        ] {
            for action in [PowerAction::Stop, PowerAction::Start] {
                rig.world.gateway.force_phase(&name, ended);
                let refused = session
                    .power(&identity, action, &CancellationToken::new())
                    .await;
                assert_ended(refused, &name, ended)?;
                assert_eq!(handle.phase(), ended);
            }
        }
        rig.world.gateway.force_phase(&name, Phase::Ready);
        assert_eq!(
            readable(
                session
                    .power(&identity, PowerAction::Stop, &CancellationToken::new())
                    .await?,
            )?
            .phase,
            Phase::Stopped
        );
        assert_eq!(
            readable(
                session
                    .power(&identity, PowerAction::Start, &CancellationToken::new())
                    .await?,
            )?
            .phase,
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
        let stop = session
            .managed_identity(&broken, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                &stop,
                Err(LifecycleError::Identity(ManagedIdentityError::Unavailable { sandbox }))
                    if *sandbox == broken
            ),
            "{stop:?}"
        );
        let handle = gateway.sandbox(&broken).ok_or("no sandbox")?;
        assert_eq!(handle.phase(), Phase::Ready);

        let sleeper = SandboxName::parse("jackin-broken-0002")?;
        gateway.insert_unmanaged(&sleeper, managed_only, Phase::Stopped)?;
        let start = session
            .managed_identity(&sleeper, &CancellationToken::new())
            .await;
        assert!(
            matches!(
                &start,
                Err(LifecycleError::Identity(ManagedIdentityError::Unavailable { sandbox }))
                    if *sandbox == sleeper
            ),
            "{start:?}"
        );
        let handle = gateway.sandbox(&sleeper).ok_or("no sandbox")?;
        assert_eq!(handle.phase(), Phase::Stopped);

        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
        let identity = session
            .managed_identity(&name, &CancellationToken::new())
            .await?;
        assert_eq!(
            readable(
                session
                    .power(&identity, PowerAction::Stop, &CancellationToken::new())
                    .await?,
            )?
            .phase,
            Phase::Stopped
        );
        assert_eq!(
            readable(
                session
                    .power(&identity, PowerAction::Start, &CancellationToken::new())
                    .await?,
            )?
            .phase,
            Phase::Ready
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancelling_the_token_ends_the_wait_for_the_sandbox_to_become_ready() -> TestResult {
    guard(async {
        let rig = rig().await?;
        rig.launch(1).await?;
        let name = rig.handle(1).await?.name();
        let session = &rig.world.session;
        let identity = session
            .managed_identity(&name, &CancellationToken::new())
            .await?;
        session
            .power(&identity, PowerAction::Stop, &CancellationToken::new())
            .await?;
        rig.world.gateway.inject(Fault::StartIgnored);
        let before = rig.world.clock.slept().len();
        let token = CancellationToken::new();
        let waiting = session.power(&identity, PowerAction::Start, &token);
        let driver = async {
            while rig.world.clock.slept().len() == before {
                tokio::task::yield_now().await;
            }
            token.cancel();
        };
        let (result, ()) = tokio::join!(waiting, driver);
        assert!(
            matches!(result, Err(LifecycleError::Cancelled)),
            "{result:?}"
        );
        let slept: Duration = rig.world.clock.slept().iter().skip(before).sum();
        assert!(slept < READY_LIMIT, "{slept:?}");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancellation_during_power_lookup_leaves_the_sandbox_ready() -> TestResult {
    guard(async {
        let rig = rig().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        let identity = session.managed_identity(&name, &CancellationToken::new()).await?;
        let token = CancellationToken::new();
        let lookup = jackin_testkit::Trigger::Op(GatewayOp::Detail);
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: lookup.clone(),
        });
        let power = session.power(&identity, PowerAction::Stop, &token);
        tokio::pin!(power);
        tokio::select! {
            held = rig.world.gateway.until_held(&lookup) => assert!(held),
            result = &mut power => return Err(format!("power lookup completed before its hold: {result:?}").into()),
        }
        token.cancel();
        let polled = std::future::poll_fn(|cx| Poll::Ready(power.as_mut().poll(cx))).await;
        assert!(rig.world.gateway.release(&lookup));
        assert!(
            matches!(polled, Poll::Ready(Err(LifecycleError::Cancelled))),
            "{polled:?}"
        );
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancellation_during_power_transition_leaves_the_sandbox_ready() -> TestResult {
    guard(async {
        let rig = rig().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        let identity = session.managed_identity(&name, &CancellationToken::new()).await?;
        let token = CancellationToken::new();
        let transition = jackin_testkit::Trigger::Op(GatewayOp::Power);
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: transition.clone(),
        });
        let power = session.power(&identity, PowerAction::Stop, &token);
        tokio::pin!(power);
        tokio::select! {
            held = rig.world.gateway.until_held(&transition) => assert!(held),
            result = &mut power => return Err(format!("power transition completed before its hold: {result:?}").into()),
        }
        token.cancel();
        let polled = std::future::poll_fn(|cx| Poll::Ready(power.as_mut().poll(cx))).await;
        assert!(rig.world.gateway.release(&transition));
        assert!(
            matches!(polled, Poll::Ready(Err(LifecycleError::Cancelled))),
            "{polled:?}"
        );
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancellation_during_power_observation_returns_cancelled_after_the_transition() -> TestResult
{
    guard(async {
        let rig = rig().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        let identity = session.managed_identity(&name, &CancellationToken::new()).await?;
        session
            .power(&identity, PowerAction::Stop, &CancellationToken::new())
            .await?;
        rig.world.gateway.inject(Fault::StartIgnored);
        let clock_guard = rig.world.clock.working();
        let token = CancellationToken::new();
        let power = session.power(&identity, PowerAction::Start, &token);
        tokio::pin!(power);
        let wait_for_sleep = async {
            while rig.world.clock.sleepers() == 0 {
                tokio::task::yield_now().await;
            }
        };
        tokio::select! {
            () = wait_for_sleep => {},
            result = &mut power => return Err(format!("power completed before observation retry: {result:?}").into()),
        }
        let trigger = jackin_testkit::Trigger::Op(GatewayOp::Detail);
        rig.world.gateway.inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        drop(clock_guard);
        tokio::select! {
            held = rig.world.gateway.until_held(&trigger) => assert!(held),
            result = &mut power => return Err(format!("power observation completed before its hold: {result:?}").into()),
        }
        token.cancel();
        let polled = std::future::poll_fn(|cx| Poll::Ready(power.as_mut().poll(cx))).await;
        assert!(rig.world.gateway.release(&trigger));
        assert!(
            matches!(polled, Poll::Ready(Err(LifecycleError::Cancelled))),
            "{polled:?}"
        );
        assert_eq!(handle.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn power_rejects_a_replacement_captured_before_the_action() -> TestResult {
    guard(async {
        for action in [PowerAction::Start, PowerAction::Stop] {
            let rig = rig().await?;
            rig.launch(1).await?;
            let name = rig.handle(1).await?.name();
            let cancel = CancellationToken::new();
            let identity = rig.world.session.managed_identity(&name, &cancel).await?;
            rig.world.gateway.delete_sandbox(&name).await?;
            rig.world.clock.advance(Duration::from_secs(60));
            rig.launch(1).await?;
            let replacement = rig.handle(1).await?;
            let phase = replacement.phase();
            let result = rig.world.session.power(&identity, action, &cancel).await;
            assert!(matches!(result, Err(LifecycleError::Identity(ManagedIdentityError::Replaced { sandbox })) if sandbox == name));
            assert_eq!(replacement.phase(), phase);
        }
        Ok(())
    }).await
}

#[tokio::test]
async fn power_rejects_a_replacement_during_observation() -> TestResult {
    guard(async {
        let rig = rig().await?;
        rig.launch(1).await?;
        let original = rig.handle(1).await?;
        let name = original.name();
        let labels = original.labels();
        let cancel = CancellationToken::new();
        let session = &rig.world.session;
        let identity = session.managed_identity(&name, &cancel).await?;
        session.power(&identity, PowerAction::Stop, &cancel).await?;
        rig.world.gateway.inject(Fault::StartIgnored);
        let held = jackin_testkit::Trigger::Op(GatewayOp::Detail);
        let clock = rig.world.clock.working();
        let power = session.power(&identity, PowerAction::Start, &cancel);
        let swap = async {
            while rig.world.clock.sleepers() == 0 {
                tokio::task::yield_now().await;
            }
            rig.world.gateway.inject(Fault::HoldAt { trigger: held.clone() });
            drop(clock);
            assert!(rig.world.gateway.until_held(&held).await);
            rig.world.gateway.delete_sandbox(&name).await?;
            rig.world.clock.advance(Duration::from_secs(60));
            rig.world.gateway.insert_unmanaged(&name, labels, Phase::Ready)?;
            rig.world.gateway.release(&held);
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (result, swapped) = tokio::join!(power, swap);
        swapped?;
        assert!(matches!(result, Err(LifecycleError::Identity(ManagedIdentityError::Replaced { sandbox })) if sandbox == name));
        assert_eq!(rig.world.gateway.sandbox(&name).ok_or("missing replacement")?.phase(), Phase::Ready);
        Ok(())
    }).await
}
