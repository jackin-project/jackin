// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AttemptId, HookKind, SupervisorState, sandbox_layout};
use jackin_session::{
    LaunchEvent, LaunchStep, LineDelivery, RestartError, StepResult, launch_events,
};
use jackin_testkit::{ProgramScript, Rig, TestResult, guard};
use tokio_util::sync::CancellationToken;

fn attempt(number: u64) -> Result<AttemptId, Box<dyn std::error::Error>> {
    Ok(AttemptId::try_from(number)?)
}

#[tokio::test]
async fn restart_refuses_a_running_agent_and_otherwise_runs_preflight_and_requests_the_next_attempt()
-> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        rig.launch(1).await?;
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
        assert_eq!(handle.runs(&preflight).len(), runs + 1);
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
