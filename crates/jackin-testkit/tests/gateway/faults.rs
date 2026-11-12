// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{HostPath, LabelSelector, LaunchId, Phase};
use jackin_openshell::{
    Gateway, GatewayError, GatewayOp, InputMode, Missing, PowerAction, Provisioning, SandboxQuery,
    SandboxView, TransferOp,
};
use jackin_testkit::{Fault, ProgramScript};
use tokio_util::sync::CancellationToken;

use super::support::{
    IMAGE, TestResult, attach, fake, guard, observe, path, program_run, read, ready, run, sh, spec,
    spec_with, supervised, write,
};

fn unreachable() -> GatewayError {
    GatewayError::Unreachable {
        endpoint: "fake".to_owned(),
        message: "injected".to_owned(),
    }
}

fn assert_failed(view: &SandboxView) -> TestResult {
    assert_eq!(view.phase, Phase::Error { exit_code: None });
    let Provisioning::Failed { message } = &view.provisioning else {
        return Err(format!("not failed: {view:?}").into());
    };
    assert!(!message.as_str().is_empty());
    Ok(())
}

#[tokio::test]
async fn provision_fails_ends_the_first_n_creates_in_error_with_a_failed_condition() -> TestResult {
    guard(async {
        let fake = fake()?;
        fake.inject(Fault::ProvisionFails { creates: 2 });
        for _ in 0..2 {
            let spec = spec(sh("sleep 3600")?)?;
            fake.create_sandbox(&spec).await?;
            for _ in 0..20 {
                let view = observe(&fake, spec.name()).await?;
                assert_failed(&view)?;
            }
            let blocked = fake
                .run_program(
                    &program_run(spec.name(), "/bin/sh", &["-c", "true"])?,
                    &CancellationToken::new(),
                )
                .await;
            assert!(matches!(
                blocked,
                Err(GatewayError::NotReady {
                    phase: Phase::Error { exit_code: None }
                })
            ));
            fake.delete_sandbox(spec.name()).await?;
        }
        let third = spec(sh("sleep 3600")?)?;
        fake.create_sandbox(&third).await?;
        let view = observe(&fake, third.name()).await?;
        assert_eq!(view.phase, Phase::Ready);
        assert!(matches!(view.provisioning, Provisioning::Normal));

        let unknown = spec_with(
            &LaunchId::generate()?,
            "jackin.invalid/unregistered:0123456789abcdef",
            Vec::new(),
            sh("sleep 3600")?,
        )?;
        fake.create_sandbox(&unknown).await?;
        assert_failed(&observe(&fake, unknown.name()).await?)?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn provision_for_keeps_provisioning_for_exactly_n_observations() -> TestResult {
    guard(async {
        let fake = fake()?;
        fake.inject(Fault::ProvisionFor { polls: 3 });
        let spec = spec(sh("sleep 3600")?)?;
        fake.create_sandbox(&spec).await?;
        let blocked = fake
            .run_program(
                &program_run(spec.name(), "/bin/sh", &["-c", "true"])?,
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(
            blocked,
            Err(GatewayError::NotReady {
                phase: Phase::Provisioning
            })
        ));
        for _ in 0..2 {
            assert_eq!(
                observe(&fake, spec.name()).await?.phase,
                Phase::Provisioning
            );
        }
        assert_eq!(observe(&fake, spec.name()).await?.phase, Phase::Ready);

        let plain = self::fake()?;
        let spec = self::spec(sh("sleep 3600")?)?;
        plain.create_sandbox(&spec).await?;
        assert_eq!(observe(&plain, spec.name()).await?.phase, Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn fail_next_and_fail_after_hit_one_call_of_one_op_and_change_no_state() -> TestResult {
    guard(async {
        let fake = fake()?;
        fake.inject(Fault::FailNext {
            op: GatewayOp::Create,
            error: unreachable(),
        });
        let spec = spec(sh("sleep 3600")?)?;
        let failed = fake.create_sandbox(&spec).await;
        assert!(failed.as_ref().is_err_and(GatewayError::is_retryable));
        assert!(fake.sandboxes().is_empty());
        fake.create_sandbox(&spec).await?;

        fake.inject(Fault::FailNext {
            op: GatewayOp::Delete,
            error: unreachable(),
        });
        observe(&fake, spec.name()).await?;
        assert!(fake.delete_sandbox(spec.name()).await.is_err());
        assert_eq!(fake.sandboxes().len(), 1);
        fake.delete_sandbox(spec.name()).await?;

        fake.inject(Fault::FailAfter {
            op: GatewayOp::Find,
            calls: 2,
            error: unreachable(),
        });
        let query = SandboxQuery::Name(spec.name().clone());
        let outcomes = [
            fake.find_sandboxes(&query).await.is_ok(),
            fake.find_sandboxes(&query).await.is_ok(),
            fake.find_sandboxes(&query).await.is_ok(),
            fake.find_sandboxes(&query).await.is_ok(),
        ];
        assert_eq!(outcomes, [true, true, false, true]);

        let handle = ready(&fake, sh("sleep 3600")?).await?;
        fake.inject(Fault::FailNext {
            op: GatewayOp::Power,
            error: GatewayError::NotFound {
                what: Missing::Sandbox(handle.name()),
            },
        });
        let denied = fake.set_power(&handle.name(), PowerAction::Stop).await;
        assert!(denied.as_ref().is_err_and(|e| !e.is_retryable()));
        assert!(matches!(denied, Err(GatewayError::NotFound { .. })));
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn resource_exhausted_terminal_fault_leaves_no_attachment_and_clears() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        fake.inject(Fault::FailNext {
            op: GatewayOp::Terminal,
            error: GatewayError::ResourceExhausted {
                what: "terminals".to_owned(),
            },
        });
        let failed = attach(&fake, &handle.name(), InputMode::Owner).await;
        assert!(
            failed
                .err()
                .is_some_and(|e| e.to_string().contains("resource exhausted"))
        );
        assert_eq!(handle.attachments(), 0);
        let _session = attach(&fake, &handle.name(), InputMode::Owner).await?;
        assert_eq!(handle.attachments(), 1);
        Ok(())
    })
    .await
}

#[tokio::test(start_paused = true)]
async fn stalled_program_times_out_with_the_runs_timeout_or_cancels() -> TestResult {
    let fake = fake()?;
    let handle = ready(&fake, sh("sleep 3600")?).await?;
    let hook = path("/usr/local/lib/jackin/hooks/setup")?;
    let other = path("/usr/local/lib/jackin/hooks/preflight")?;
    let script = ProgramScript {
        exit_code: 0,
        stdout: b"ok".to_vec(),
        stderr: Vec::new(),
    };
    fake.script_program(&hook, script.clone());
    fake.script_program(&other, script);
    fake.inject(Fault::StallProgram {
        program: hook.clone(),
    });
    let marker = path("/sandbox/marker")?;
    let mut stalled = program_run(&handle.name(), hook.as_str(), &[])?;
    stalled.timeout = Duration::from_secs(5);
    stalled.touch_on_success = Some(marker);
    for _ in 0..2 {
        let before = tokio::time::Instant::now();
        let result = fake.run_program(&stalled, &CancellationToken::new()).await;
        assert_eq!(before.elapsed(), Duration::from_secs(5));
        assert!(matches!(
            result,
            Err(GatewayError::Timeout { op: GatewayOp::Program, after, .. }) if after == Duration::from_secs(5)
        ));
    }
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });
    let result = fake.run_program(&stalled, &cancel).await;
    assert!(matches!(
        result,
        Err(GatewayError::Cancelled {
            op: GatewayOp::Program,
            ..
        })
    ));
    let free = run(&fake, &program_run(&handle.name(), other.as_str(), &[])?).await?;
    assert_eq!(free.stdout, b"ok");
    assert_eq!(read(&fake, &handle.name(), "/sandbox/marker").await?, None);
    Ok(())
}

#[tokio::test]
async fn corrupt_download_fails_with_a_digest_transfer_error() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        write(&fake, &handle.name(), "/sandbox/export.bin", "bundle-bytes").await?;
        let dir = tempfile::tempdir()?;
        let local = HostPath::parse(&dir.path().join("export.bin"))?;
        let remote = path("/sandbox/export.bin")?;
        let cancel = CancellationToken::new();
        fake.inject(Fault::CorruptDownload);
        let corrupted = fake
            .download_file(&handle.name(), &remote, &local, &cancel)
            .await;
        assert!(matches!(
            corrupted,
            Err(GatewayError::Transfer {
                op: TransferOp::Digest,
                ..
            })
        ));
        fake.download_file(&handle.name(), &remote, &local, &cancel)
            .await?;
        assert_eq!(tokio::fs::read(local.as_path()).await?, b"bundle-bytes");
        Ok(())
    })
    .await
}

#[tokio::test]
async fn hide_until_create_conflict_hides_label_finds_until_a_create_meets_the_existing_name()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let launch = LaunchId::generate()?;
        let sandbox = spec_with(&launch, IMAGE, Vec::new(), sh("sleep 3600")?)?;
        fake.create_sandbox(&sandbox).await?;
        let selector = SandboxQuery::Labels(LabelSelector::launch(&launch));
        assert_eq!(fake.find_sandboxes(&selector).await?.len(), 1);
        fake.inject(Fault::HideUntilCreateConflict);
        for _ in 0..3 {
            assert!(fake.find_sandboxes(&selector).await?.is_empty());
        }
        assert_eq!(observe(&fake, sandbox.name()).await?.name, *sandbox.name());
        let conflict = fake.create_sandbox(&sandbox).await;
        assert!(
            matches!(conflict, Err(GatewayError::AlreadyExists { what: Missing::Sandbox(ref found) }) if found == sandbox.name()),
            "{conflict:?}"
        );
        assert_eq!(fake.find_sandboxes(&selector).await?.len(), 1);
        assert_eq!(fake.sandboxes().len(), 1);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn start_ignored_accepts_the_start_and_leaves_the_sandbox_stopped() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, supervised()?).await?;
        let name = handle.name();
        fake.set_power(&name, PowerAction::Stop).await?;
        fake.inject(Fault::StartIgnored);
        for _ in 0..3 {
            fake.set_power(&name, PowerAction::Start).await?;
            assert_eq!(observe(&fake, &name).await?.phase, Phase::Stopped);
        }
        assert_eq!(handle.phase(), Phase::Stopped);
        Ok(())
    })
    .await
}
