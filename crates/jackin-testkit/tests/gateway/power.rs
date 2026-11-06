// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use jackin_core::{LaunchId, Phase, ProjectRoot, SandboxName};
use jackin_openshell::{
    Gateway, GatewayError, InputMode, PowerAction, Provisioning, TerminalKind, TerminalRequest,
    TransferOp,
};
use jackin_testkit::{FakeGateway, Fault};
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, fake, guard, main_request, observe, path, program_run, project, read, ready, sh,
    size, spec, spec_with, write,
};

const MISSING_IMAGE: &str = "jackin.invalid/unregistered:0123456789abcdef";

#[tokio::test]
async fn stop_on_a_stopped_sandbox_is_ok_keeps_the_files_and_ended_sandboxes_still_refuse_it()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/kept", "data").await?;

        let stopped = fake.set_power(&name, PowerAction::Stop, &CancellationToken::new()).await?;
        assert_eq!(stopped.phase, Phase::Stopped);
        let again = fake.set_power(&name, PowerAction::Stop, &CancellationToken::new()).await?;
        assert_eq!(again.phase, Phase::Stopped);
        for _ in 0..5 {
            assert_eq!(observe(&fake, &name).await?.phase, Phase::Stopped);
        }
        assert_eq!(handle.phase(), Phase::Stopped);
        assert_eq!(handle.file(&path("/sandbox/kept")?), Some(b"data".to_vec()));
        assert_eq!(handle.attachments(), 0);

        for (code, ended) in [
            (7, Phase::Error { exit_code: Some(7) }),
            (0, Phase::Completed { exit_code: 0 }),
        ] {
            let handle = ready(&fake, sh("sleep 3600")?).await?;
            handle.exit_main(code);
            let stop = fake.set_power(&handle.name(), PowerAction::Stop, &CancellationToken::new()).await;
            assert!(
                matches!(&stop, Err(GatewayError::MainProcessFailed { sandbox }) if *sandbox == handle.name()),
                "{stop:?}"
            );
            assert_eq!(handle.phase(), ended);
        }

        let handle = ready(&fake, sh("sleep 3600")?).await?;
        handle.exit_main(7);
        let start = fake.set_power(&handle.name(), PowerAction::Start, &CancellationToken::new()).await;
        assert!(matches!(
            start,
            Err(GatewayError::FailedPrecondition { .. })
        ));
        assert_eq!(handle.phase(), Phase::Starting);
        let stop = fake.set_power(&handle.name(), PowerAction::Stop, &CancellationToken::new()).await;
        assert!(
            matches!(stop, Err(GatewayError::FailedPrecondition { .. })),
            "{stop:?}"
        );
        Ok(())
    })
    .await
}

async fn failed_provisioning(
    fake: &FakeGateway,
    image: &str,
) -> Result<SandboxName, Box<dyn std::error::Error>> {
    let spec = spec_with(&LaunchId::generate()?, image, Vec::new(), sh("sleep 3600")?)?;
    fake.create_sandbox(&spec).await?;
    Ok(spec.name().clone())
}

async fn assert_unchanged_failure(fake: &FakeGateway, name: &SandboxName) -> TestResult {
    let view = observe(fake, name).await?;
    assert_eq!(view.phase, Phase::Error { exit_code: None });
    assert!(
        matches!(view.provisioning, Provisioning::Failed { .. }),
        "{view:?}"
    );
    Ok(())
}

#[tokio::test]
async fn stop_and_start_on_a_provisioning_failed_sandbox_follow_the_p4_4_capture() -> TestResult {
    guard(async {
        for unregistered in [true, false] {
            let fake = fake()?;
            let image = if unregistered {
                MISSING_IMAGE
            } else {
                super::support::IMAGE
            };
            if !unregistered {
                fake.inject(Fault::ProvisionFails { creates: 1 });
            }
            let name = failed_provisioning(&fake, image).await?;
            assert_unchanged_failure(&fake, &name).await?;
            let handle = fake.sandbox(&name).ok_or("not listed")?;

            for _ in 0..2 {
                let stop = fake.set_power(&name, PowerAction::Stop, &CancellationToken::new()).await;
                assert!(
                    matches!(&stop, Err(GatewayError::Protocol { message }) if message.contains("stop sandbox failed")),
                    "{stop:?}"
                );
                assert_eq!(handle.phase(), Phase::Error { exit_code: None });
            }
            for _ in 0..10 {
                assert_unchanged_failure(&fake, &name).await?;
            }
            for _ in 0..2 {
                let start = fake.set_power(&name, PowerAction::Start, &CancellationToken::new()).await;
                assert!(
                    matches!(start, Err(GatewayError::FailedPrecondition { .. })),
                    "{start:?}"
                );
                assert_eq!(handle.phase(), Phase::Error { exit_code: None });
            }
            for _ in 0..10 {
                assert_unchanged_failure(&fake, &name).await?;
            }
            let main = fake
                .open_terminal(&main_request(&name, InputMode::Owner)?)
                .await;
            assert!(matches!(
                main,
                Err(GatewayError::NotReady {
                    phase: Phase::Error { exit_code: None }
                })
            ));

            fake.delete_sandbox(&name).await?;
            assert!(fake.sandboxes().is_empty());
        }
        Ok(())
    })
    .await
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Program,
    Read,
    Write,
    Upload,
    Shell,
    Main,
    Stop,
}

const OPS: [Op; 7] = [
    Op::Program,
    Op::Read,
    Op::Write,
    Op::Upload,
    Op::Shell,
    Op::Main,
    Op::Stop,
];

async fn call(
    fake: &FakeGateway,
    name: &SandboxName,
    source: &ProjectRoot,
    op: Op,
) -> Result<Result<(), GatewayError>, Box<dyn std::error::Error>> {
    let outcome = match op {
        Op::Program => fake
            .run_program(
                &program_run(name, "/bin/sh", &["-c", "true"])?,
                &CancellationToken::new(),
            )
            .await
            .map(drop),
        Op::Read => fake
            .read_files(name, &[path("/sandbox/kept")?], &CancellationToken::new())
            .await
            .map(drop),
        Op::Write => {
            let file = jackin_openshell::SandboxFile {
                path: path("/sandbox/new")?,
                mode: jackin_core::FileMode(0o600),
                content: jackin_core::SecretBytes::new(b"x".to_vec()),
            };
            fake.write_file(name, &file, &CancellationToken::new())
                .await
        }
        Op::Upload => {
            fake.upload_tree(
                name,
                source,
                &path("/sandbox/project")?,
                &CancellationToken::new(),
            )
            .await
        }
        Op::Shell => fake
            .open_terminal(&TerminalRequest {
                sandbox: name.clone(),
                kind: TerminalKind::Shell {
                    workdir: path("/sandbox")?,
                    env: BTreeMap::new(),
                },
                size: size(100, 30)?,
            })
            .await
            .map(drop),
        Op::Main => fake
            .open_terminal(&main_request(name, InputMode::Owner)?)
            .await
            .map(drop),
        Op::Stop => fake
            .set_power(name, PowerAction::Stop, &CancellationToken::new())
            .await
            .map(drop),
    };
    Ok(outcome)
}

fn early_error_fits(op: Op, error: &GatewayError) -> bool {
    match op {
        Op::Upload => matches!(
            error,
            GatewayError::Transfer {
                op: TransferOp::UploadFiltered,
                ..
            }
        ),
        Op::Stop => matches!(error, GatewayError::FailedPrecondition { .. }),
        _ => matches!(
            error,
            GatewayError::NotReady {
                phase: Phase::Provisioning
            }
        ),
    }
}

#[tokio::test]
async fn every_phase_reading_call_is_one_observation_that_settles_a_provisioning_window()
-> TestResult {
    guard(async {
        let (_dir, root) = project().await?;
        for op in OPS {
            let fake = fake()?;
            fake.inject(Fault::ProvisionFor { polls: 2 });
            let spec = spec(sh("sleep 3600")?)?;
            let name = spec.name().clone();
            fake.create_sandbox(&spec).await?;
            for number in 1..=2 {
                let result = call(&fake, &name, &root, op).await?;
                assert!(
                    result
                        .as_ref()
                        .is_err_and(|error| early_error_fits(op, error)),
                    "{op:?} call {number}: {result:?}"
                );
            }
            let third = call(&fake, &name, &root, op).await?;
            assert!(third.is_ok(), "{op:?} call 3: {third:?}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_start_settles_on_the_first_call_that_reads_the_phase_and_the_files_are_kept()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/kept", "data").await?;
        fake.set_power(&name, PowerAction::Stop, &CancellationToken::new())
            .await?;
        fake.set_power(&name, PowerAction::Start, &CancellationToken::new())
            .await?;
        assert_eq!(handle.phase(), Phase::Starting);
        assert_eq!(
            read(&fake, &name, "/sandbox/kept").await?,
            Some(b"data".to_vec())
        );
        assert_eq!(handle.phase(), Phase::Ready);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_failed_call_does_not_settle_and_handles_and_absent_sandboxes_never_observe() -> TestResult
{
    guard(async {
        let fake = fake()?;
        fake.inject(Fault::ProvisionFor { polls: 1 });
        let spec = spec(sh("sleep 3600")?)?;
        let name = spec.name().clone();
        fake.create_sandbox(&spec).await?;
        let handle = fake.sandbox(&name).ok_or("not listed")?;
        fake.inject(Fault::FailNext {
            op: jackin_openshell::GatewayOp::Program,
            error: GatewayError::Unreachable {
                endpoint: "fake".to_owned(),
                message: "injected".to_owned(),
            },
        });
        let run = program_run(&name, "/bin/sh", &["-c", "true"])?;
        let cancel = CancellationToken::new();
        let first = fake.run_program(&run, &cancel).await;
        assert!(
            matches!(first, Err(GatewayError::Unreachable { .. })),
            "{first:?}"
        );
        let second = fake.run_program(&run, &cancel).await;
        assert!(
            matches!(
                second,
                Err(GatewayError::NotReady {
                    phase: Phase::Provisioning
                })
            ),
            "{second:?}"
        );
        for _ in 0..5 {
            assert_eq!(handle.phase(), Phase::Provisioning);
        }
        let third = fake.run_program(&run, &cancel).await?;
        assert_eq!(third.exit_code, 0);

        let absent = spec_with(
            &LaunchId::generate()?,
            super::support::IMAGE,
            Vec::new(),
            sh("true")?,
        )?;
        let missing = fake
            .run_program(
                &program_run(absent.name(), "/bin/sh", &["-c", "true"])?,
                &cancel,
            )
            .await;
        assert!(
            matches!(missing, Err(GatewayError::NotFound { .. })),
            "{missing:?}"
        );
        Ok(())
    })
    .await
}
