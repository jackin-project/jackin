// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{AttemptId, HookKind, SupervisorState, sandbox_layout};
use jackin_session::{LaunchError, LaunchStep};

use super::support::{TestResult, failure, guard, launch_rig, ready, running, write};
use jackin_testkit::SupervisorModel;

fn preflight() -> jackin_core::SandboxPath {
    sandbox_layout::hook_path(HookKind::Preflight)
}

#[tokio::test]
async fn attempt_ids_follow_the_state_and_nothing_is_written_while_running() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_eq!(handle.supervisor(), Some(running(1)?));

        handle.exit_codex(3);
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Exited {
                attempt: AttemptId::FIRST,
                code: 3
            })
        );
        let target = ready(rig.launch_observed(1).await?)?;
        assert_eq!(target.state, running(2)?);
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"2".to_vec()));
        assert_eq!(handle.runs(&preflight()).len(), 2);
        assert_eq!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Setup))
                .len(),
            1
        );

        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        handle.exit_codex(0);
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Exited {
                attempt: AttemptId::FIRST,
                code: 0
            })
        );
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));

        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        write(
            &rig,
            &handle.name(),
            &sandbox_layout::RUN_STATE,
            "exited 999999999999999999 0\n",
        )
        .await?;
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Start);
        assert!(
            matches!(&*failed.error, LaunchError::AttemptsExhausted { .. }),
            "{:?}",
            failed.error
        );
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_silent_or_invalid_supervisor_times_out_before_any_hook_or_start() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway().set_supervisor(SupervisorModel::NeverWritesState);
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Supervisor);
        assert!(
            matches!(&*failed.error, LaunchError::SupervisorTimeout { after, .. } if *after >= std::time::Duration::from_secs(20)),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        assert!(handle.runs(&preflight()).is_empty());
        assert!(handle.runs(&sandbox_layout::hook_path(HookKind::Setup)).is_empty());
        assert!(handle.file(&sandbox_layout::RUN_START).is_none());

        let rig = launch_rig().await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        write(&rig, &handle.name(), &sandbox_layout::RUN_STATE, "banana\n").await?;
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Supervisor);
        assert!(matches!(&*failed.error, LaunchError::SupervisorTimeout { .. }), "{:?}", failed.error);
        assert_eq!(handle.file(&sandbox_layout::RUN_START), Some(b"1".to_vec()));
        assert_eq!(handle.runs(&preflight()).len(), 1);

        let rig = launch_rig().await?;
        rig.gateway().set_supervisor(SupervisorModel::ExitsOnStart { code: 3 });
        let target = ready(rig.launch_observed(1).await?)?;
        assert_eq!(
            target.state,
            SupervisorState::Exited { attempt: AttemptId::FIRST, code: 3 }
        );
        Ok(())
    })
    .await
}
