// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{ProfileId, ProviderName, SandboxLabels, SupervisorState, sandbox_layout};
use jackin_image::BuildStage;
use jackin_openshell::{GatewayError, GatewayOp};
use jackin_session::{LaunchError, LaunchOutcome, LaunchStep};

use super::support::{
    TestResult, converged, failure, guard, has_file, launch_id, launch_rig, path, ready, running,
    under_project, unreachable, write,
};
use jackin_testkit::{Fault, Trigger};

#[tokio::test]
async fn two_runs_with_one_launch_id_give_one_build_one_sandbox_one_upload_one_setup() -> TestResult
{
    guard(async {
        let rig = launch_rig().await?;
        let first = ready(rig.launch_observed(1).await?)?;
        rig.repo.file("late.txt", b"late\n").await?;
        let second = ready(rig.launch_observed(1).await?)?;
        let name = rig.plan(1)?.sandbox_name().clone();
        assert_eq!(name.as_str(), "jackin-platfor-ij4f");
        for target in [first, second] {
            assert_eq!(target.sandbox, name);
            assert_eq!(
                target.state,
                SupervisorState::Running {
                    attempt: running_attempt()
                }
            );
        }
        assert_eq!(
            rig.stages(),
            [BuildStage::Construct, BuildStage::Role, BuildStage::Derived]
        );
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        let built = rig.final_image()?;
        let expected = rig
            .plan(1)?
            .draft()
            .labels()
            .clone()
            .complete(&built.id, jackin_core::DockerLabel::Off);
        assert_eq!(SandboxLabels::from_labels(&handle.labels())?, expected);
        for present in ["src/main.rs", "README.md", ".git/HEAD", "notes.txt"] {
            assert!(
                has_file(&handle, &path(&format!("/sandbox/project/{present}"))?),
                "{present}"
            );
        }
        for absent in ["target/out.bin", "late.txt"] {
            assert!(
                !has_file(&handle, &path(&format!("/sandbox/project/{absent}"))?),
                "{absent}"
            );
        }
        converged(&rig, &handle, 1)?;
        assert_eq!(
            handle
                .runs(&sandbox_layout::hook_path(jackin_core::HookKind::Setup))
                .len(),
            1
        );
        assert_eq!(
            handle
                .runs(&sandbox_layout::hook_path(jackin_core::HookKind::Preflight))
                .len(),
            1
        );
        assert_eq!(
            rig.gateway().providers(),
            [ProviderName::parse("jackin-work-codex")?]
        );
        assert_eq!(rig.gateway().profiles(), [ProfileId::parse("codex")?]);
        Ok(())
    })
    .await
}

fn running_attempt() -> jackin_core::AttemptId {
    jackin_core::AttemptId::FIRST
}

#[tokio::test]
async fn a_failure_after_create_leaves_the_sandbox_and_a_retry_continues_it() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let auth = jackin_core::Agent::Codex.spec().auth_file.path.clone();
        rig.gateway().inject(Fault::FailAt {
            trigger: Trigger::Write(auth.clone()),
            error: unreachable(),
        });
        let failed = failure(rig.launch_observed(1).await?)?;
        let name = rig.plan(1)?.sandbox_name().clone();
        assert_eq!(failed.launch_id, launch_id(1));
        assert_eq!(failed.step, LaunchStep::Credentials);
        assert_eq!(failed.sandbox, Some(name.clone()));
        assert!(matches!(
            *failed.error,
            LaunchError::Gateway(GatewayError::Protocol { .. })
        ));
        let handle = rig.handle(1).await?;
        assert_eq!(handle.phase(), jackin_core::Phase::Ready);
        let built = rig.final_image()?;
        let labels = rig
            .plan(1)?
            .draft()
            .labels()
            .clone()
            .complete(&built.id, jackin_core::DockerLabel::Off);
        assert_eq!(SandboxLabels::from_labels(&handle.labels())?, labels);
        assert!(!has_file(
            &handle,
            &sandbox_layout::uploaded_marker(&launch_id(1))
        ));
        assert!(!under_project(&handle));
        assert!(!has_file(&handle, &sandbox_layout::RUN_START));
        assert!(
            handle
                .runs(&sandbox_layout::hook_path(jackin_core::HookKind::Setup))
                .is_empty()
        );
        assert!(
            handle
                .runs(&sandbox_layout::hook_path(jackin_core::HookKind::Preflight))
                .is_empty()
        );

        write(&rig, &name, &path("/sandbox/sentinel.txt")?, "s").await?;
        let retried = rig.launch_observed(1).await?;
        assert!(matches!(retried.result, Ok(LaunchOutcome::Ready(_))));
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        assert_eq!(
            handle.file(&path("/sandbox/sentinel.txt")?),
            Some(b"s".to_vec())
        );
        converged(&rig, &handle, 1)?;
        assert_eq!(running(1)?, handle.supervisor().ok_or("no state")?);
        Ok(())
    })
    .await
}

const FINDS_BEFORE_CREATE: u32 = 3;

#[tokio::test]
async fn a_failure_right_after_create_still_reports_the_created_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().inject(Fault::FailAfter {
            op: GatewayOp::Find,
            calls: FINDS_BEFORE_CREATE,
            error: unreachable(),
        });
        let failed = failure(rig.launch_observed(1).await?)?;
        let name = rig.plan(1)?.sandbox_name().clone();
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert_eq!(failed.sandbox, Some(name));
        assert!(matches!(
            *failed.error,
            LaunchError::Gateway(GatewayError::Protocol { .. })
        ));
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failure_right_after_a_join_still_reports_the_joined_sandbox() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let first = ready(rig.launch_observed(1).await?)?;
        rig.gateway().inject(Fault::HideUntilCreateConflict);
        rig.gateway().inject(Fault::FailAfter {
            op: GatewayOp::Find,
            calls: FINDS_BEFORE_CREATE + 1,
            error: unreachable(),
        });
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Sandbox);
        assert_eq!(failed.sandbox, Some(first.sandbox));
        assert!(matches!(
            *failed.error,
            LaunchError::Gateway(GatewayError::Protocol { .. })
        ));
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn two_concurrent_launches_with_one_id_end_with_one_sandbox_and_one_start() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let (first, second) = tokio::join!(rig.launch_observed(1), rig.launch_observed(1));
        let (first, second) = (ready(first?)?, ready(second?)?);
        assert_eq!(first.sandbox, second.sandbox);
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        assert_eq!(handle.supervisor(), Some(running(1)?));
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_run_that_loses_the_create_race_joins_the_sandbox_of_its_own_launch() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let first = ready(rig.launch_observed(1).await?)?;
        rig.gateway().inject(Fault::HideUntilCreateConflict);
        let second = ready(rig.launch_observed(1).await?)?;
        assert_eq!(second.sandbox, first.sandbox);
        assert_eq!(rig.gateway().sandboxes().len(), 1);
        let handle = rig.handle(1).await?;
        assert_eq!(handle.supervisor(), Some(running(1)?));
        converged(&rig, &handle, 1)?;
        Ok(())
    })
    .await
}
