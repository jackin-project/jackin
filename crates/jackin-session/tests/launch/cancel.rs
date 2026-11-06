// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, HookKind, sandbox_layout};
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{LaunchOutcome, RestoreCandidate, RestoreFinding};
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, converged, guard, has_file, launch_id, launch_rig, ready, running,
};
use jackin_testkit::{FakeGateway, Fault, ProgramScript, Rig, Trigger};
use std::sync::Arc;

struct LookupRelease {
    gateway: Arc<FakeGateway>,
    trigger: Trigger,
}

impl Drop for LookupRelease {
    fn drop(&mut self) {
        self.gateway.release(&self.trigger);
    }
}

#[derive(Clone, Copy, Debug)]
enum Point {
    Before,
    Provider,
    Create,
    Probe,
    Auth,
    Upload,
    Setup,
    Start,
}

fn trigger(point: Point) -> Option<Trigger> {
    let spec = Agent::Codex.spec();
    match point {
        Point::Before => None,
        Point::Provider => Some(Trigger::Op(GatewayOp::Provider)),
        Point::Create => Some(Trigger::Op(GatewayOp::Create)),
        Point::Probe => Some(Trigger::Op(GatewayOp::ReadFiles)),
        Point::Auth => Some(Trigger::Write(spec.auth_file.path.clone())),
        Point::Upload => Some(Trigger::Op(GatewayOp::UploadTree)),
        Point::Setup => Some(Trigger::Program(sandbox_layout::hook_path(HookKind::Setup))),
        Point::Start => Some(Trigger::Write(sandbox_layout::RUN_START.clone())),
    }
}

fn assert_nothing_before_sandbox(rig: &Rig, point: Point) {
    if matches!(point, Point::Before) {
        assert!(rig.world.engine.builds().is_empty());
    } else {
        assert!(rig.gateway().providers().is_empty());
    }
}

async fn cancel_at(point: Point) -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        if matches!(point, Point::Create) {
            rig.gateway().inject(Fault::CancelAfterCreate {
                token: token.clone(),
            });
        } else {
            match trigger(point) {
                Some(trigger) => rig.gateway().inject(Fault::CancelAt {
                    trigger,
                    token: token.clone(),
                }),
                None => token.cancel(),
            }
        }
        let outcome = rig.run(rig.request(1)?, token).await?;
        let Ok(LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: id,
            finding,
        })) = outcome.result
        else {
            return Err(format!("{point:?}: not cancelled: {:?}", outcome.result).into());
        };
        assert_eq!(id, launch_id(1));
        let found = rig.sandbox(1).await?;
        let expected = match &found {
            Some(handle) => RestoreFinding::Observed(handle.name()),
            None => RestoreFinding::Unconfirmed {
                last_observed: None,
            },
        };
        assert_eq!(finding, expected, "{point:?}");
        assert!(rig.gateway().sandboxes().len() <= 1);
        let setup = sandbox_layout::hook_path(HookKind::Setup);
        match (point, &found) {
            (Point::Before | Point::Provider, found) => {
                assert!(found.is_none(), "{point:?}");
                assert!(rig.gateway().sandboxes().is_empty());
                assert_nothing_before_sandbox(&rig, point);
            }
            (Point::Create | Point::Probe, Some(handle)) => {
                assert!(!has_file(handle, &Agent::Codex.spec().auth_file.path));
            }
            (Point::Auth, Some(handle)) => {
                assert!(has_file(handle, &Agent::Codex.spec().auth_file.path));
                assert!(!has_file(handle, &Agent::Codex.spec().user_config.path));
            }
            (Point::Upload, Some(handle)) => {
                assert!(!has_file(
                    handle,
                    &sandbox_layout::uploaded_marker(&launch_id(1))
                ));
                assert!(!has_file(handle, &sandbox_layout::RUN_START));
            }
            (Point::Setup, Some(handle)) => {
                assert!(handle.runs(&setup).is_empty());
                assert!(!has_file(
                    handle,
                    &sandbox_layout::setup_done_marker(&launch_id(1))
                ));
                assert!(!has_file(handle, &sandbox_layout::RUN_START));
            }
            (Point::Start, Some(handle)) => {
                assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
            }
            (point, None) => return Err(format!("{point:?}: no sandbox").into()),
        }
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        converged(&rig, &handle, 1)?;
        assert_eq!(handle.runs(&setup).len(), 1);
        if matches!(point, Point::Start) {
            assert_eq!(handle.supervisor(), Some(running(1)?));
            assert_eq!(
                handle
                    .runs(&sandbox_layout::hook_path(HookKind::Preflight))
                    .len(),
                1
            );
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_before_the_launch_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult
{
    cancel_at(Point::Before).await
}

#[tokio::test]
async fn a_cancel_at_the_provider_sync_leaves_a_restore_candidate_and_a_retry_completes()
-> TestResult {
    cancel_at(Point::Provider).await
}

#[tokio::test]
async fn a_cancel_at_the_create_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult {
    cancel_at(Point::Create).await
}

#[tokio::test]
async fn an_empty_recovery_read_after_create_is_unconfirmed_and_retry_finds_the_committed_sandbox()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::HideUntilCreateConflict);
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::CancelAfterCreate {
            token: token.clone(),
        });
        let cancelled = rig.run(rig.request(1)?, token).await?.result?;
        let LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: id,
            finding,
        }) = cancelled
        else {
            return Err(format!("unexpected outcome: {cancelled:?}").into());
        };
        assert_eq!(id, launch_id(1));
        assert_eq!(
            finding,
            RestoreFinding::Unconfirmed {
                last_observed: Some(rig.sandbox_name(1)?),
            }
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        ready(rig.launch_observed(1).await?)?;
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_held_recovery_lookup_returns_unconfirmed_at_its_deadline() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let trigger = Trigger::Op(GatewayOp::Find);
        rig.gateway().inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        let token = CancellationToken::new();
        token.cancel();
        let planned = rig.request(1)?;
        let request = rig.run(planned, token);
        tokio::pin!(request);
        let working = rig.world.clock.working();
        let release_lookup = LookupRelease {
            gateway: Arc::clone(rig.gateway()),
            trigger: trigger.clone(),
        };
        tokio::select! {
            held = rig.gateway().until_held(&trigger) => assert!(held),
            outcome = &mut request => return Err(format!("launch completed before recovery lookup held: {outcome:?}").into()),
        }
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while rig.world.clock.sleepers() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .map_err(|_| std::io::Error::other("the recovery deadline sleeper was not registered"))?;
        rig.world
            .clock
            .advance(std::time::Duration::from_secs(3));
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), &mut request)
            .await
            .map_err(|_| std::io::Error::other("recovery remained pending after its deadline"))?;
        let cancelled = outcome?.result?;
        let LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: id,
            finding,
        }) = cancelled
        else {
            return Err(format!("unexpected outcome: {cancelled:?}").into());
        };
        assert_eq!(id, launch_id(1));
        assert_eq!(
            finding,
            RestoreFinding::Unconfirmed {
                last_observed: None,
            }
        );
        drop(release_lookup);
        drop(working);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_pre_dispatch_cancel_does_not_claim_that_a_sandbox_is_absent() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        token.cancel();
        let cancelled = rig.run(rig.request(1)?, token).await?.result?;
        let LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: id,
            finding,
        }) = cancelled
        else {
            return Err(format!("unexpected outcome: {cancelled:?}").into());
        };
        assert_eq!(id, launch_id(1));
        assert_eq!(
            finding,
            RestoreFinding::Unconfirmed {
                last_observed: None,
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn multiple_recovery_matches_are_unconfirmed_instead_of_selecting_one() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.launch(1).await?;
        rig.gateway().inject(Fault::DuplicateNextFind);
        let token = CancellationToken::new();
        token.cancel();
        let cancelled = rig.run(rig.request(1)?, token).await?.result?;
        let LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: id,
            finding,
        }) = cancelled
        else {
            return Err(format!("unexpected outcome: {cancelled:?}").into());
        };
        assert_eq!(id, launch_id(1));
        assert_eq!(
            finding,
            RestoreFinding::Unconfirmed {
                last_observed: None,
            }
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn recovery_failure_keeps_only_the_last_observed_sandbox_as_unconfirmed() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let token = CancellationToken::new();
        let trigger = Trigger::Write(Agent::Codex.spec().auth_file.path.clone());
        rig.gateway().inject(Fault::HoldAt {
            trigger: trigger.clone(),
        });
        rig.gateway().inject(Fault::CancelAt {
            trigger: trigger.clone(),
            token: token.clone(),
        });
        let request = rig.run(rig.request(1)?, token);
        let recovery = async {
            assert!(rig.gateway().until_held(&trigger).await);
            rig.gateway().inject(Fault::FailNext {
                op: GatewayOp::Find,
                error: GatewayError::Protocol {
                    message: "recovery unavailable".to_owned(),
                },
            });
            assert!(rig.gateway().release(&trigger));
        };
        let (outcome, ()) = tokio::join!(request, recovery);
        let cancelled = outcome?.result?;
        let LaunchOutcome::Cancelled(RestoreCandidate {
            launch_id: id,
            finding,
        }) = cancelled
        else {
            return Err(format!("unexpected outcome: {cancelled:?}").into());
        };
        assert_eq!(id, launch_id(1));
        assert_eq!(
            finding,
            RestoreFinding::Unconfirmed {
                last_observed: Some(rig.sandbox_name(1)?),
            }
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_at_the_probe_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult {
    cancel_at(Point::Probe).await
}

#[tokio::test]
async fn a_cancel_at_the_credentials_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult
{
    cancel_at(Point::Auth).await
}

#[tokio::test]
async fn a_cancel_at_the_upload_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult {
    cancel_at(Point::Upload).await
}

#[tokio::test]
async fn a_cancel_at_the_setup_hook_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult
{
    cancel_at(Point::Setup).await
}

#[tokio::test]
async fn a_cancel_at_the_start_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult {
    cancel_at(Point::Start).await
}

#[tokio::test]
async fn a_cancel_stops_a_running_hook_and_the_retry_runs_it_once() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let setup = sandbox_layout::hook_path(HookKind::Setup);
        let token = CancellationToken::new();
        rig.gateway().inject(Fault::StallProgram {
            program: setup.clone(),
        });
        rig.gateway().inject(Fault::CancelAt {
            trigger: Trigger::Program(setup.clone()),
            token: token.clone(),
        });
        let outcome = rig.run(rig.request(1)?, token).await?;
        assert!(
            matches!(outcome.result, Ok(LaunchOutcome::Cancelled(_))),
            "{:?}",
            outcome.result
        );
        let handle = rig.handle(1).await?;
        assert!(handle.runs(&setup).is_empty());
        assert!(!has_file(
            &handle,
            &sandbox_layout::setup_done_marker(&launch_id(1))
        ));
        assert!(!has_file(&handle, &sandbox_layout::RUN_START));

        rig.gateway().release_stall(&setup);
        rig.gateway().script_program(
            &setup,
            ProgramScript {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        assert_eq!(handle.runs(&setup).len(), 1);
        converged(&rig, &handle, 1)?;
        Ok(())
    })
    .await
}
