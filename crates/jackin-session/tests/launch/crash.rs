// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, HookKind, SandboxPath, sandbox_layout};
use jackin_image::{BuildStage, EngineError, ImageError};
use jackin_openshell::GatewayOp;
use jackin_session::{LaunchError, LaunchStep};

use super::support::{
    TestResult, actual_final_builds, converged, failure, guard, has_file, launch_id, launch_rig,
    path, ready, under_project, unreachable,
};
use jackin_testkit::{DockerBuildReply, Fault, Trigger};

fn setup() -> SandboxPath {
    sandbox_layout::hook_path(HookKind::Setup)
}

fn preflight() -> SandboxPath {
    sandbox_layout::hook_path(HookKind::Preflight)
}

#[tokio::test]
async fn a_failure_before_the_sandbox_exists_builds_and_creates_nothing_twice() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let (_, builds, _) = actual_final_builds(&rig, 1).await?;
        rig.world.docker.script_build(
            builds.derived.tag.clone(),
            DockerBuildReply::failed(b"injected external Buildx failure"),
        );
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Image);
        assert!(matches!(
            *failed.error,
            LaunchError::Image(ImageError::Engine(EngineError::BuildFailed {
                stage: BuildStage::Derived,
                ..
            }))
        ));
        assert_eq!(failed.sandbox, None);
        assert!(rig.gateway().sandboxes().is_empty());
        assert!(rig.gateway().providers().is_empty());
        assert_eq!(rig.world.docker.image_id(&builds.derived.tag), None);
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        converged(&rig, &handle, 1)?;
        assert!(rig.world.docker.image_id(&builds.derived.tag).is_some());

        for (op, step, providers) in [
            (GatewayOp::Provider, LaunchStep::Provider, 0),
            (GatewayOp::Create, LaunchStep::Sandbox, 1),
        ] {
            let rig = launch_rig().await?;
            let (_, builds, _) = actual_final_builds(&rig, 1).await?;
            rig.gateway().inject(Fault::FailAt {
                trigger: Trigger::Op(op),
                error: unreachable(),
            });
            let failed = failure(rig.launch_observed(1).await?)?;
            let images = rig.world.docker.images();
            assert_eq!(failed.step, step);
            assert_eq!(failed.sandbox, None);
            assert!(rig.gateway().sandboxes().is_empty());
            assert_eq!(rig.gateway().providers().len(), providers);
            assert!(rig.world.docker.image_id(&builds.derived.tag).is_some());
            ready(rig.launch_observed(1).await?)?;
            converged(&rig, &rig.handle(1).await?, 1)?;
            assert_eq!(rig.world.docker.images(), images);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failure_in_credentials_or_upload_resumes_without_repeating_finished_steps() -> TestResult
{
    guard(async {
        let config = Agent::Codex.spec().user_config.path.clone();
        let marker = sandbox_layout::uploaded_marker(&launch_id(1));
        let rows = [
            (Trigger::Write(config.clone()), LaunchStep::Credentials),
            (Trigger::Op(GatewayOp::UploadTree), LaunchStep::Upload),
            (Trigger::Write(marker.clone()), LaunchStep::Upload),
        ];
        for (index, (trigger, step)) in rows.into_iter().enumerate() {
            let rig = launch_rig().await?;
            rig.gateway().inject(Fault::FailAt {
                trigger,
                error: unreachable(),
            });
            let failed = failure(rig.launch_observed(1).await?)?;
            assert_eq!(failed.step, step);
            assert!(failed.sandbox.is_some());
            let handle = rig.handle(1).await?;
            assert_eq!(handle.phase(), jackin_core::Phase::Ready);
            assert!(handle.runs(&setup()).is_empty());
            assert!(handle.runs(&preflight()).is_empty());
            assert!(!has_file(&handle, &sandbox_layout::RUN_START));
            assert!(!has_file(
                &handle,
                &sandbox_layout::setup_done_marker(&launch_id(1))
            ));
            match index {
                0 => {
                    assert!(!has_file(&handle, &marker));
                    assert!(!under_project(&handle));
                }
                1 => {
                    assert!(has_file(&handle, &config));
                    assert!(!under_project(&handle));
                    assert!(!has_file(&handle, &marker));
                }
                _ => {
                    assert!(under_project(&handle));
                    assert!(!has_file(&handle, &marker));
                    rig.repo.file("late.txt", b"late\n").await?;
                }
            }
            ready(rig.launch_observed(1).await?)?;
            let handle = rig.handle(1).await?;
            converged(&rig, &handle, 1)?;
            let plan = rig.plan(1)?;
            assert_eq!(
                handle.file(&config),
                Some(plan.files().codex_config.text.clone().into_bytes())
            );
            assert!(has_file(&handle, &Agent::Codex.spec().auth_file.path));
            assert!(has_file(&handle, &plan.files().env.path));
            if index == 2 {
                assert!(has_file(&handle, &path("/sandbox/project/late.txt")?));
            }
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failure_in_hooks_or_start_resumes_without_repeating_setup() -> TestResult {
    guard(async {
        let rows = [
            (Trigger::Program(setup()), 0_usize),
            (Trigger::Program(preflight()), 1),
            (Trigger::Write(sandbox_layout::RUN_START.clone()), 2),
        ];
        for (trigger, index) in rows {
            let rig = launch_rig().await?;
            rig.gateway().inject(Fault::FailAt {
                trigger,
                error: unreachable(),
            });
            let failed = failure(rig.launch_observed(1).await?)?;
            let handle = rig.handle(1).await?;
            let marker = sandbox_layout::setup_done_marker(&launch_id(1));
            assert_eq!(
                failed.step,
                if index == 2 {
                    LaunchStep::Start
                } else {
                    LaunchStep::Hooks
                }
            );
            assert_eq!(
                handle.supervisor(),
                Some(jackin_core::SupervisorState::Waiting)
            );
            assert_eq!(has_file(&handle, &marker), index != 0);
            assert!(!has_file(&handle, &sandbox_layout::RUN_START));
            assert_eq!(handle.runs(&setup()).len(), usize::from(index != 0));
            assert_eq!(handle.runs(&preflight()).len(), usize::from(index == 2));
            ready(rig.launch_observed(1).await?)?;
            let handle = rig.handle(1).await?;
            converged(&rig, &handle, 1)?;
            assert_eq!(handle.runs(&setup()).len(), 1);
            assert_eq!(
                handle.runs(&preflight()).len(),
                if index == 2 { 2 } else { 1 }
            );
        }
        Ok(())
    })
    .await
}
