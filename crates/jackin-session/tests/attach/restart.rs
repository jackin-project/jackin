// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{AttemptId, EnvName, HookKind, Phase, SupervisorState, sandbox_layout};
use jackin_session::{
    LaunchEvent, LaunchStep, LineDelivery, RestartError, StepResult, launch_events,
};
use jackin_testkit::{ProgramScript, Rig, TestResult, default_limits, guard};
use tokio_util::sync::CancellationToken;

use super::tap::{Tap, file};

fn attempt(number: u64) -> Result<AttemptId, Box<dyn std::error::Error>> {
    Ok(AttemptId::try_from(number)?)
}

#[tokio::test]
async fn restart_refuses_a_running_agent_and_otherwise_runs_preflight_and_requests_the_next_attempt()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let env = BTreeMap::from([(EnvName::parse("GREETING")?, "from-launch".to_owned())]);
        rig.launch_with_env(1, env.clone()).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let session = &rig.world.session;
        let preflight = sandbox_layout::hook_path(HookKind::Preflight);
        let runs = handle.runs(&preflight).len();

        let (events, _feed) = launch_events(LineDelivery::Reliable);
        let busy = session
            .restart_agent(&name, events, CancellationToken::new())
            .await;
        assert!(
            matches!(
                busy,
                Err(RestartError::AlreadyRunning { attempt: seen, .. }) if seen == AttemptId::FIRST
            ),
            "{busy:?}"
        );
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_eq!(handle.runs(&preflight).len(), runs);

        handle.exit_codex(3);
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let target = session
            .restart_agent(&name, events, CancellationToken::new())
            .await?;
        assert_eq!(
            target.state,
            SupervisorState::Running {
                attempt: attempt(2)?
            }
        );
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"2".to_vec()));
        let after = handle.runs(&preflight);
        assert_eq!(after.len(), runs + 1);
        assert_eq!(after.last().map(|run| &run.env), Some(&env));
        let mut seen = Vec::new();
        while let Some(event) = feed.recv().await {
            seen.push(event);
        }
        assert_eq!(
            seen,
            vec![
                LaunchEvent::StepStarted(LaunchStep::Hooks),
                LaunchEvent::StepFinished {
                    step: LaunchStep::Hooks,
                    result: StepResult::Done
                },
                LaunchEvent::StepStarted(LaunchStep::Start),
                LaunchEvent::StepFinished {
                    step: LaunchStep::Start,
                    result: StepResult::Done
                },
            ]
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failing_preflight_stops_the_restart_before_any_request_is_written() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        handle.exit_codex(3);
        rig.world.gateway.script_program(
            &sandbox_layout::hook_path(HookKind::Preflight),
            ProgramScript {
                exit_code: 4,
                stdout: b"no\n".to_vec(),
                stderr: Vec::new(),
            },
        );

        let (events, _feed) = launch_events(LineDelivery::Reliable);
        let failed = rig
            .world
            .session
            .restart_agent(&handle.name(), events, CancellationToken::new())
            .await;
        assert!(
            matches!(
                failed,
                Err(RestartError::HookFailed {
                    hook: HookKind::Preflight,
                    exit_code: 4,
                    ..
                })
            ),
            "{failed:?}"
        );
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Exited {
                attempt: AttemptId::FIRST,
                code: 3
            })
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_restart_on_a_sandbox_whose_main_process_ended_is_main_process_ended_and_writes_nothing()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        handle.exit_codex(3);
        let preflight = sandbox_layout::hook_path(HookKind::Preflight);
        let runs = handle.runs(&preflight).len();
        for ended in [
            Phase::Completed { exit_code: 0 },
            Phase::Error { exit_code: Some(7) },
            Phase::Error { exit_code: None },
        ] {
            rig.world.gateway.force_phase(&name, ended);
            let (events, _feed) = launch_events(LineDelivery::Reliable);
            let refused = rig
                .world
                .session
                .restart_agent(&name, events, CancellationToken::new())
                .await;
            match refused {
                Err(RestartError::MainProcessEnded { sandbox, phase }) => {
                    assert_eq!((sandbox, phase), (name.clone(), ended));
                }
                other => return Err(format!("{ended:?}: saw {other:?}").into()),
            }
        }
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_eq!(handle.runs(&preflight).len(), runs);
        Ok(())
    })
    .await
}

async fn restart_with_state_written_after_preflight(
    state: &str,
) -> Result<
    (
        Result<jackin_session::AttachTarget, RestartError>,
        Option<Vec<u8>>,
    ),
    Box<dyn std::error::Error>,
> {
    let rig = Rig::new().await?;
    rig.launch(1).await?;
    let handle = rig.handle(1).await?;
    handle.exit_codex(3);
    let tap = Tap::new(&rig);
    tap.after_program(
        &sandbox_layout::hook_path(HookKind::Preflight),
        file(&sandbox_layout::RUN_STATE, state.as_bytes()),
    );
    let session = tap.session(&rig, default_limits());
    let (events, _feed) = launch_events(LineDelivery::Reliable);
    let result = session
        .restart_agent(&handle.name(), events, CancellationToken::new())
        .await;
    Ok((result, handle.file(&sandbox_layout::RUN_START)))
}

#[tokio::test]
async fn a_state_change_between_preflight_and_the_write_is_refused_or_restarts_from_the_fresh_attempt()
-> TestResult {
    guard(async {
        let (running, start) = restart_with_state_written_after_preflight("running 2\n").await?;
        assert!(
            matches!(&running, Err(RestartError::AlreadyRunning { attempt: seen, .. }) if *seen == attempt(2)?),
            "{running:?}"
        );
        assert_eq!(start, Some(b"1".to_vec()));

        let (garbage, start) = restart_with_state_written_after_preflight("garbage\n").await?;
        assert!(
            matches!(garbage, Err(RestartError::StateUnreadable { .. })),
            "{garbage:?}"
        );
        assert_eq!(start, Some(b"1".to_vec()));

        let (exited, start) = restart_with_state_written_after_preflight("exited 5 0\n").await?;
        let target = exited?;
        assert_eq!(
            target.state,
            SupervisorState::Running {
                attempt: attempt(6)?
            }
        );
        assert_eq!(start, Some(b"6".to_vec()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn concurrent_restarts_of_one_sandbox_are_serialized_by_a_per_sandbox_lock() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
        rig.launch(2).await?;
        let first = rig.handle(1).await?;
        let second = rig.handle(2).await?;
        first.exit_codex(3);
        second.exit_codex(3);
        let preflight = sandbox_layout::hook_path(HookKind::Preflight);
        let runs = first.runs(&preflight).len();
        let tap = Tap::new(&rig);
        let gate = tap.gate_program(&preflight);
        let session = tap.session(&rig, default_limits());

        let (events, _feed_a) = launch_events(LineDelivery::Reliable);
        let a_session = session.clone();
        let a_name = first.name();
        let holder = tokio::spawn(async move {
            a_session
                .restart_agent(&a_name, events, CancellationToken::new())
                .await
        });
        gate.entered.notified().await;

        let (events, _feed_b) = launch_events(LineDelivery::Reliable);
        let busy = session
            .restart_agent(&first.name(), events, CancellationToken::new())
            .await;
        assert!(
            matches!(&busy, Err(RestartError::Busy { sandbox }) if *sandbox == first.name()),
            "{busy:?}"
        );

        let (events, _feed_c) = launch_events(LineDelivery::Reliable);
        let other = session
            .restart_agent(&second.name(), events, CancellationToken::new())
            .await?;
        assert_eq!(
            other.state,
            SupervisorState::Running {
                attempt: attempt(2)?
            }
        );

        gate.release.notify_one();
        let done = holder.await??;
        assert_eq!(
            done.state,
            SupervisorState::Running {
                attempt: attempt(2)?
            }
        );
        assert_eq!(first.file(&sandbox_layout::RUN_START), Some(b"2".to_vec()));
        assert_eq!(first.runs(&preflight).len(), runs + 1);

        first.exit_codex(1);
        rig.world.gateway.script_program(
            &preflight,
            ProgramScript {
                exit_code: 4,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        for _ in 0..2 {
            let (events, _feed) = launch_events(LineDelivery::Reliable);
            let failed = session
                .restart_agent(&first.name(), events, CancellationToken::new())
                .await;
            assert!(
                matches!(failed, Err(RestartError::HookFailed { exit_code: 4, .. })),
                "{failed:?}"
            );
        }
        Ok(())
    })
    .await
}
