// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_image::{BuildStage, ImageEvent};
use jackin_openshell::GatewayOp;
use jackin_session::{LaunchEvent, LaunchOutcome, LaunchStep, SkipReason, StepResult};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, launch_rig, ready, unreachable};
use jackin_testkit::{Fault, Outcome, Trigger};

fn checked(events: &[LaunchEvent]) -> Result<Vec<(LaunchStep, StepResult)>, String> {
    let mut results = Vec::new();
    let mut open = None;
    let mut next = 0;
    let mut known = 0;
    for event in events {
        match event {
            LaunchEvent::StepStarted(step) => {
                if open.is_some() || *step != LaunchStep::ALL[next] {
                    return Err(format!("{step:?} started out of order"));
                }
                open = Some(*step);
                next += 1;
            }
            LaunchEvent::StepFinished { step, result } => {
                if open != Some(*step) {
                    return Err(format!("{step:?} finished while {open:?} was open"));
                }
                open = None;
                results.push((*step, *result));
            }
            LaunchEvent::Image(_) => {
                if open != Some(LaunchStep::Image) {
                    return Err(format!("an image event outside the image step: {open:?}"));
                }
            }
            LaunchEvent::SandboxKnown(_) => {
                known += 1;
                if open.is_some() || next > 4 {
                    return Err("SandboxKnown at the wrong place".to_owned());
                }
            }
            LaunchEvent::Warning(_) => return Err("a warning".to_owned()),
        }
    }
    if open.is_some() || results.len() != 9 || known != 1 {
        return Err(format!(
            "{} steps, {known} known, {open:?} open",
            results.len()
        ));
    }
    Ok(results)
}

fn images(events: &[LaunchEvent]) -> Vec<ImageEvent> {
    events
        .iter()
        .filter_map(|event| match event {
            LaunchEvent::Image(image) => Some(image.clone()),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn events_come_in_step_order_with_skips_named() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let first = rig.launch_observed(1).await?;
        let results = checked(&first.events)?;
        assert!(
            results
                .iter()
                .all(|(_, result)| *result == StepResult::Done),
            "{results:?}"
        );
        assert_eq!(
            images(&first.events),
            [
                ImageEvent::StageStarted(BuildStage::Construct),
                ImageEvent::StageFinished(BuildStage::Construct),
                ImageEvent::StageStarted(BuildStage::Role),
                ImageEvent::StageFinished(BuildStage::Role),
                ImageEvent::StageStarted(BuildStage::Derived),
                ImageEvent::StageFinished(BuildStage::Derived),
            ]
        );
        assert!(first.events.contains(&LaunchEvent::SandboxKnown(
            rig.plan(1)?.sandbox_name().clone()
        )));
        ready(first)?;

        let second = rig.launch_observed(1).await?;
        let results = checked(&second.events)?;
        let skipped = |reason| StepResult::Skipped(reason);
        assert_eq!(
            results,
            [
                (LaunchStep::Image, skipped(SkipReason::ImageCached)),
                (LaunchStep::Provider, skipped(SkipReason::ProviderPresent)),
                (LaunchStep::Sandbox, skipped(SkipReason::SandboxExists)),
                (LaunchStep::Ready, StepResult::Done),
                (LaunchStep::Credentials, StepResult::Done),
                (LaunchStep::Upload, skipped(SkipReason::AlreadyUploaded)),
                (LaunchStep::Supervisor, skipped(SkipReason::AlreadyRunning)),
                (LaunchStep::Hooks, skipped(SkipReason::AlreadyRunning)),
                (LaunchStep::Start, skipped(SkipReason::AlreadyRunning)),
            ]
        );
        assert_eq!(
            images(&second.events),
            [
                ImageEvent::StageCached(BuildStage::Construct),
                ImageEvent::StageCached(BuildStage::Derived),
            ]
        );
        Ok(())
    })
    .await
}

fn last(outcome: &Outcome) -> Option<&LaunchEvent> {
    outcome.events.last()
}

#[tokio::test]
async fn a_failed_or_cancelled_run_ends_its_events_at_the_failing_step() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().script_program(
            &jackin_core::sandbox_layout::hook_path(jackin_core::HookKind::Preflight),
            jackin_testkit::ProgramScript {
                exit_code: 1,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        let failed = rig.launch_observed(1).await?;
        assert!(matches!(&failed.result, Err(failure) if failure.step == LaunchStep::Hooks));
        assert_eq!(
            last(&failed),
            Some(&LaunchEvent::StepStarted(LaunchStep::Hooks))
        );

        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            token: token.clone(),
        });
        let cancelled = rig.run(rig.request(1)?, token).await?;
        assert!(matches!(cancelled.result, Ok(LaunchOutcome::Cancelled(_))));
        assert_eq!(
            last(&cancelled),
            Some(&LaunchEvent::StepStarted(LaunchStep::Upload))
        );

        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::Create),
            error: unreachable(),
        });
        let refused = rig.launch_observed(1).await?;
        assert_eq!(
            last(&refused),
            Some(&LaunchEvent::StepStarted(LaunchStep::Sandbox))
        );
        for outcome in [&failed, &cancelled, &refused] {
            let finished: Vec<LaunchStep> = outcome
                .events
                .iter()
                .filter_map(|event| match event {
                    LaunchEvent::StepFinished { step, .. } => Some(*step),
                    _ => None,
                })
                .collect();
            let started: Vec<LaunchStep> = outcome
                .events
                .iter()
                .filter_map(|event| match event {
                    LaunchEvent::StepStarted(step) => Some(*step),
                    _ => None,
                })
                .collect();
            assert_eq!(finished.len() + 1, started.len());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_recreated_sandbox_keeps_the_events_forward_only() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::ProvisionFails { creates: 2 });
        let run = rig.launch_observed(1).await?;
        checked(&run.events)?;
        ready(run)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_ended_sandbox_enters_the_ready_step_before_the_run_fails() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        rig.handle(1).await?.exit_main(3);
        let run = rig.launch_observed(1).await?;
        assert!(matches!(&run.result, Err(failure) if failure.step == LaunchStep::Ready));
        assert_eq!(
            last(&run),
            Some(&LaunchEvent::StepStarted(LaunchStep::Ready))
        );
        assert!(!run.events.iter().any(|event| matches!(
            event,
            LaunchEvent::StepFinished {
                step: LaunchStep::Ready,
                ..
            }
        )));
        Ok(())
    })
    .await
}
