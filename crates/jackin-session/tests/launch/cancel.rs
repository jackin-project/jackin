// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, HookKind, sandbox_layout};
use jackin_openshell::GatewayOp;
use jackin_session::{LaunchOutcome, RestoreCandidate};
use jackin_testkit::{Fault, ProgramScript, Trigger};
use tokio_util::sync::CancellationToken;

use super::support::{Rig, TestResult, converged, guard, has_file, launch_id, ready, running};

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

const POINTS: [Point; 8] = [
    Point::Before,
    Point::Provider,
    Point::Create,
    Point::Probe,
    Point::Auth,
    Point::Upload,
    Point::Setup,
    Point::Start,
];

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

#[tokio::test]
async fn a_cancel_at_each_point_leaves_a_restore_candidate_and_a_retry_completes() -> TestResult {
    guard(async {
        for point in POINTS {
            let rig = Rig::new().await?;
            let token = CancellationToken::new();
            match trigger(point) {
                Some(trigger) => rig.gateway().inject(Fault::CancelAt {
                    trigger,
                    token: token.clone(),
                }),
                None => token.cancel(),
            }
            let outcome = rig.run(rig.request(1)?, token).await?;
            let Ok(LaunchOutcome::Cancelled(RestoreCandidate {
                launch_id: id,
                sandbox,
            })) = outcome.result
            else {
                return Err(format!("{point:?}: not cancelled: {:?}", outcome.result).into());
            };
            assert_eq!(id, launch_id(1));
            let found = rig.sandbox(1).await?;
            assert_eq!(
                sandbox,
                found.as_ref().map(jackin_testkit::SandboxHandle::name),
                "{point:?}"
            );
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
            ready(rig.launch(1).await?)?;
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
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_cancel_stops_a_running_hook_and_the_retry_runs_it_once() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
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
        ready(rig.launch(1).await?)?;
        let handle = rig.handle(1).await?;
        assert_eq!(handle.runs(&setup).len(), 1);
        converged(&rig, &handle, 1)?;
        Ok(())
    })
    .await
}
