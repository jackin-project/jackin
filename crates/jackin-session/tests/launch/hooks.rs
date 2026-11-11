// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    EnvName, FileLock, HookKind, HostPath, Phase, PlainEnvValue, SupervisorState, sandbox_layout,
};
use jackin_openshell::{Gateway, ProgramRun, ProgramStdin};
use jackin_session::{
    LaunchError, LaunchEvent, LaunchFeed, LaunchOutcome, LaunchStep, LineDelivery, SkipReason,
    StepResult, launch_events,
};
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, builder, converged, failure, guard, has_file, launch_id, launch_rig, path, ready,
    write,
};
use jackin_testkit::{ProgramScript, Rig, RoleFixture};

fn script(rig: &Rig, kind: HookKind, exit_code: i32, stdout: &[u8], stderr: &[u8]) {
    rig.gateway().script_program(
        &sandbox_layout::hook_path(kind),
        ProgramScript {
            exit_code,
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        },
    );
}

#[tokio::test]
async fn a_failing_hook_stops_the_launch_with_name_code_and_sanitized_output() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        script(
            &rig,
            HookKind::Preflight,
            7,
            b"checking\x1b[31m red \x1b]0;title\x07done\n",
            b"boom\n",
        );
        let failed = failure(rig.launch_observed(1).await?)?;
        assert_eq!(failed.step, LaunchStep::Hooks);
        assert!(failed.sandbox.is_some());
        let LaunchError::HookFailed {
            hook: HookKind::Preflight,
            exit_code: 7,
            output,
        } = &*failed.error
        else {
            return Err(format!("{:?}", failed.error).into());
        };
        for wanted in ["checking", "red", "done", "boom"] {
            assert!(output.as_str().contains(wanted), "{wanted}");
        }
        assert!(!output.as_str().contains('\x1b') && !output.as_str().contains('\x07'));
        let handle = rig.handle(1).await?;
        assert_eq!(handle.phase(), Phase::Ready);
        let probe = ProgramRun {
            sandbox: handle.name(),
            program: path("/usr/bin/git")?,
            args: vec!["--version".to_owned()],
            workdir: None,
            env: std::collections::BTreeMap::new(),
            stdin: ProgramStdin::Closed,
            touch_on_success: None,
            timeout: std::time::Duration::from_secs(30),
        };
        assert_eq!(
            rig.gateway()
                .run_program(&probe, &CancellationToken::new())
                .await?
                .exit_code,
            0
        );
        assert!(
            rig.gateway()
                .read_files(
                    &handle.name(),
                    std::slice::from_ref(&sandbox_layout::RUN_STATE)
                )
                .await?[0]
                .is_some()
        );
        assert_eq!(handle.supervisor(), Some(SupervisorState::Waiting));
        assert!(!has_file(&handle, &sandbox_layout::RUN_START));
        assert!(has_file(
            &handle,
            &sandbox_layout::setup_done_marker(&launch_id(1))
        ));

        let rig = launch_rig().await?;
        script(&rig, HookKind::Setup, 3, b"", b"setup broke\n");
        let failed = failure(rig.launch_observed(1).await?)?;
        assert!(
            matches!(
                &*failed.error,
                LaunchError::HookFailed {
                    hook: HookKind::Setup,
                    exit_code: 3,
                    ..
                }
            ),
            "{:?}",
            failed.error
        );
        let handle = rig.handle(1).await?;
        assert!(!has_file(
            &handle,
            &sandbox_layout::setup_done_marker(&launch_id(1))
        ));
        assert!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Preflight))
                .is_empty()
        );
        script(&rig, HookKind::Setup, 0, b"", b"");
        ready(rig.launch_observed(1).await?)?;
        converged(&rig, &rig.handle(1).await?, 1)?;

        let rig = builder()
            .await?
            .role(RoleFixture::new("sentinel")?)
            .build()
            .await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        for kind in [HookKind::Setup, HookKind::Preflight] {
            assert!(handle.runs(&sandbox_layout::hook_path(kind)).is_empty());
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn hooks_run_with_the_env_file_values_and_a_retry_replaces_them() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        let env = |value: &str| -> Result<_, Box<dyn std::error::Error>> {
            Ok(std::collections::BTreeMap::from([(
                EnvName::parse("GREETING")?,
                PlainEnvValue::parse(value)?,
            )]))
        };
        let texts = |value: &str| -> Result<_, Box<dyn std::error::Error>> {
            Ok(std::collections::BTreeMap::from([(
                EnvName::parse("GREETING")?,
                value.to_owned(),
            )]))
        };
        script(&rig, HookKind::Preflight, 1, b"", b"");
        let first = rig.request(1)?.env(env("first")?);
        let failed = failure(rig.run(first, CancellationToken::new()).await?)?;
        assert!(matches!(
            &*failed.error,
            LaunchError::HookFailed {
                hook: HookKind::Preflight,
                exit_code: 1,
                ..
            }
        ));
        let handle = rig.handle(1).await?;
        let setups = handle.runs(&sandbox_layout::hook_path(HookKind::Setup));
        assert_eq!(setups.len(), 1);
        assert_eq!(setups[0].env, texts("first")?);
        assert_eq!(setups[0].workdir, Some(sandbox_layout::PROJECT.clone()));

        script(&rig, HookKind::Preflight, 0, b"", b"");
        let second = rig.request(1)?.env(env("second")?);
        ready(rig.run(second, CancellationToken::new()).await?)?;
        let plan = rig.plan(1)?;
        let file = handle.file(&plan.files().env.path).ok_or("no env file")?;
        assert!(String::from_utf8(file)?.contains("second"));
        let preflights = handle.runs(&sandbox_layout::hook_path(HookKind::Preflight));
        assert_eq!(preflights.len(), 2);
        assert_eq!(preflights[1].env, texts("second")?);
        assert_eq!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Setup))
                .len(),
            1
        );
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn an_installed_hook_with_no_bytes_is_absent_and_never_runs() -> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.gateway()
            .install_file(&sandbox_layout::hook_path(HookKind::Setup), Vec::new());
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        assert!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Setup))
                .is_empty()
        );
        assert!(!has_file(
            &handle,
            &sandbox_layout::setup_done_marker(&launch_id(1))
        ));
        assert_eq!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Preflight))
                .len(),
            1
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_role_with_only_a_preflight_hook_never_writes_the_setup_marker() -> TestResult {
    guard(async {
        let rig = builder()
            .await?
            .role(RoleFixture::new("sentinel")?.hook(HookKind::Preflight, b"#!/bin/sh\n"))
            .build()
            .await?;
        ready(rig.launch_observed(1).await?)?;
        let handle = rig.handle(1).await?;
        assert_eq!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Preflight))
                .len(),
            1
        );
        assert!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Setup))
                .is_empty()
        );
        assert!(!has_file(
            &handle,
            &sandbox_layout::setup_done_marker(&launch_id(1))
        ));
        Ok(())
    })
    .await
}

async fn as_a_peer(
    rig: &Rig,
    feed: &mut LaunchFeed,
    held: &mut Option<FileLock>,
) -> Result<Option<StepResult>, Box<dyn std::error::Error>> {
    let mut hooks = None;
    while let Some(event) = feed.recv().await {
        match event {
            LaunchEvent::StepStarted(LaunchStep::Hooks) => {
                let name = rig.handle(1).await?.name();
                let marker = sandbox_layout::setup_done_marker(&launch_id(1));
                write(rig, &name, &marker, "").await?;
                *held = None;
            }
            LaunchEvent::StepFinished {
                step: LaunchStep::Hooks,
                result,
            } => hooks = Some(result),
            _ => {}
        }
    }
    Ok(hooks)
}

#[tokio::test]
async fn a_launch_that_waits_for_the_hook_lock_skips_a_setup_a_peer_finished_meanwhile()
-> TestResult {
    guard(async {
        let rig = launch_rig().await?;
        rig.world.fs.create_dir_all(rig.world.cache_dir()).await?;
        let lock = HostPath::resolve(
            Path::new(&format!("hooks-{}.lock", launch_id(1))),
            rig.world.cache_dir(),
        )?;
        let mut held = Some(rig.world.fs.lock(&lock).await?);
        let (events, mut feed) = launch_events(LineDelivery::Reliable);
        let launched = rig
            .world
            .session
            .launch(rig.plan(1)?, events, CancellationToken::new());
        let peer = as_a_peer(&rig, &mut feed, &mut held);
        let (outcome, hooks) = tokio::join!(launched, peer);
        assert!(
            matches!(outcome, Ok(LaunchOutcome::Ready(_))),
            "{outcome:?}"
        );
        assert_eq!(
            hooks?,
            Some(StepResult::Skipped(SkipReason::SetupAlreadyDone))
        );
        assert!(held.is_none());
        let handle = rig.handle(1).await?;
        assert!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Setup))
                .is_empty()
        );
        assert_eq!(
            handle
                .runs(&sandbox_layout::hook_path(HookKind::Preflight))
                .len(),
            1
        );
        Ok(())
    })
    .await
}
