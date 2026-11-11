// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use jackin_core::ProjectRoot;
use jackin_core::{Phase, SandboxName, SupervisorState};
use jackin_openshell::{Gateway, GatewayError, GatewayOp, PowerAction, SandboxQuery};
use jackin_testkit::{FakeGateway, Fault, ProgramScript, Trigger};
use tokio_util::sync::CancellationToken;

use super::support::{
    TestResult, fake, guard, observe, path, program_run, project, read, ready, run, sh, spec,
    supervised, write,
};

const STATE: &str = "/sandbox/.jackin/run/state";

fn injected() -> GatewayError {
    GatewayError::Protocol {
        message: "injected".to_owned(),
    }
}

fn is_injected<T: std::fmt::Debug>(result: Result<T, GatewayError>) -> bool {
    matches!(result, Err(GatewayError::Protocol { message }) if message == "injected")
}

async fn upload() -> Result<(tempfile::TempDir, ProjectRoot), Box<dyn Error>> {
    project().await
}

#[tokio::test]
async fn trigger_faults_fire_once_at_entry_of_the_matching_call_only() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/a.txt", "old").await?;
        fake.inject(Fault::FailAt {
            trigger: Trigger::Write(path("/sandbox/a.txt")?),
            error: injected(),
        });
        write(&fake, &name, "/sandbox/b.txt", "b").await?;
        assert_eq!(
            read(&fake, &name, "/sandbox/b.txt").await?,
            Some(b"b".to_vec())
        );
        let failed = write(&fake, &name, "/sandbox/a.txt", "new").await;
        assert!(
            matches!(&failed, Err(error) if error.to_string().contains("injected")),
            "{failed:?}"
        );
        assert_eq!(
            read(&fake, &name, "/sandbox/a.txt").await?,
            Some(b"old".to_vec())
        );
        write(&fake, &name, "/sandbox/a.txt", "new").await?;
        assert_eq!(
            read(&fake, &name, "/sandbox/a.txt").await?,
            Some(b"new".to_vec())
        );

        let program = path("/usr/local/lib/jackin/hooks/setup")?;
        fake.script_program(
            &program,
            ProgramScript {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        fake.inject(Fault::FailAt {
            trigger: Trigger::Program(program.clone()),
            error: injected(),
        });
        let other = program_run(&name, "/bin/sh", &["-c", "true"])?;
        assert_eq!(run(&fake, &other).await?.exit_code, 0);
        let hook = program_run(&name, program.as_str(), &[])?;
        assert!(is_injected(
            fake.run_program(&hook, &CancellationToken::new()).await
        ));
        assert!(handle.runs(&program).is_empty());
        assert_eq!(run(&fake, &hook).await?.exit_code, 0);
        assert_eq!(handle.runs(&program).len(), 1);

        fake.inject(Fault::FailAt {
            trigger: Trigger::Op(GatewayOp::Create),
            error: injected(),
        });
        let second = spec(sh("sleep 3600")?)?;
        assert!(is_injected(fake.create_sandbox(&second).await));
        assert_eq!(fake.sandboxes().len(), 1);
        fake.create_sandbox(&second).await?;
        assert_eq!(fake.sandboxes().len(), 2);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn cancel_triggers_cancel_their_token_and_let_the_matching_call_run() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        let (_dir, source) = upload().await?;
        let token = CancellationToken::new();
        fake.inject(Fault::CancelAt {
            trigger: Trigger::Op(GatewayOp::UploadTree),
            token: token.clone(),
        });
        assert!(!token.is_cancelled());
        fake.upload_tree(
            &name,
            &source,
            &path("/sandbox/project")?,
            &CancellationToken::new(),
        )
        .await?;
        assert!(token.is_cancelled());
        assert!(handle.file(&path("/sandbox/project/README.md")?).is_some());

        let again = CancellationToken::new();
        fake.inject(Fault::CancelAt {
            trigger: Trigger::Write(path("/sandbox/c.txt")?),
            token: again.clone(),
        });
        write(&fake, &name, "/sandbox/d.txt", "d").await?;
        assert!(!again.is_cancelled());
        write(&fake, &name, "/sandbox/c.txt", "c").await?;
        assert!(again.is_cancelled());
        assert_eq!(handle.file(&path("/sandbox/c.txt")?), Some(b"c".to_vec()));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_program_run_with_a_cancelled_token_ends_cancelled_and_records_nothing() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        fake.script_program(
            &path("/bin/sh")?,
            ProgramScript {
                exit_code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        );
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let request = program_run(&handle.name(), "/bin/sh", &["-c", "true"])?;
        let result = fake.run_program(&request, &cancelled).await;
        assert!(matches!(
            result,
            Err(GatewayError::Cancelled {
                op: GatewayOp::Program,
                ..
            })
        ));
        assert!(handle.runs(&path("/bin/sh")?).is_empty());
        Ok(())
    })
    .await
}

async fn settled_state(
    fake: &FakeGateway,
    name: &SandboxName,
) -> Result<(Phase, String), Box<dyn Error>> {
    let view = observe(fake, name).await?;
    let handle = fake.sandbox(name).ok_or("the sandbox is gone")?;
    let text = handle.file(&path(STATE)?).unwrap_or_default();
    Ok((view.phase, String::from_utf8(text)?))
}

#[tokio::test]
async fn a_stale_supervisor_window_keeps_the_old_state_for_n_observations_after_power_start()
-> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, supervised()?).await?;
        let name = handle.name();
        write(&fake, &name, "/sandbox/.jackin/run/start", "1").await?;
        assert_eq!(
            handle.supervisor(),
            Some(SupervisorState::Running {
                attempt: jackin_core::AttemptId::FIRST
            })
        );
        fake.set_power(&name, PowerAction::Stop).await?;
        fake.inject(Fault::StaleSupervisor { observations: 2 });
        fake.set_power(&name, PowerAction::Start).await?;
        for _ in 0..2 {
            assert_eq!(
                settled_state(&fake, &name).await?,
                (Phase::Ready, "running 1\n".to_owned())
            );
        }
        assert_eq!(
            settled_state(&fake, &name).await?,
            (Phase::Ready, "waiting\n".to_owned())
        );

        fake.inject(Fault::StaleSupervisor { observations: 0 });
        write(&fake, &name, "/sandbox/.jackin/run/start", "1").await?;
        fake.set_power(&name, PowerAction::Stop).await?;
        fake.set_power(&name, PowerAction::Start).await?;
        assert_eq!(
            settled_state(&fake, &name).await?,
            (Phase::Ready, "waiting\n".to_owned())
        );
        Ok(())
    })
    .await
}

#[tokio::test]
async fn a_stale_window_does_not_apply_to_a_first_boot() -> TestResult {
    guard(async {
        let fake = fake()?;
        fake.inject(Fault::StaleSupervisor { observations: 3 });
        let handle = ready(&fake, supervised()?).await?;
        let (phase, text) = settled_state(&fake, &handle.name()).await?;
        assert_eq!((phase, text.as_str()), (Phase::Ready, "waiting\n"));
        Ok(())
    })
    .await
}

#[tokio::test]
async fn force_phase_install_file_and_the_files_snapshot_behave_as_documented() -> TestResult {
    guard(async {
        let fake = fake()?;
        let handle = ready(&fake, sh("sleep 3600")?).await?;
        let name = handle.name();
        fake.force_phase(&name, Phase::Suspended);
        let listed = fake
            .find_sandboxes(&SandboxQuery::Name(name.clone()))
            .await?;
        assert_eq!(listed[0].phase, Phase::Suspended);
        let blocked = fake
            .run_program(
                &program_run(&name, "/bin/sh", &["-c", "true"])?,
                &CancellationToken::new(),
            )
            .await;
        assert!(matches!(
            blocked,
            Err(GatewayError::NotReady {
                phase: Phase::Suspended
            })
        ));
        fake.force_phase(&name, Phase::Ready);

        let hook = path("/usr/local/lib/jackin/hooks/setup")?;
        assert_eq!(read(&fake, &name, hook.as_str()).await?, None);
        fake.install_file(&hook, b"x".to_vec());
        assert_eq!(
            read(&fake, &name, hook.as_str()).await?,
            Some(b"x".to_vec())
        );
        assert_eq!(
            read(&fake, &name, "/usr/local/lib/jackin/hooks/preflight").await?,
            None
        );

        write(&fake, &name, "/sandbox/one.txt", "1").await?;
        write(&fake, &name, "/sandbox/dir/two.txt", "22").await?;
        let files = handle.files();
        let expected = [
            (path("/sandbox/one.txt")?, b"1".to_vec()),
            (path("/sandbox/dir/two.txt")?, b"22".to_vec()),
        ];
        assert_eq!(files.len(), expected.len(), "{:?}", files.keys());
        for (file, bytes) in &expected {
            assert_eq!(files.get(file), Some(bytes));
        }

        let mut request = program_run(&name, "/bin/sh", &["-c", "true"])?;
        request.workdir = Some(path("/sandbox/dir")?);
        run(&fake, &request).await?;
        let records = handle.runs(&path("/bin/sh")?);
        assert_eq!(records[0].args, ["-c", "true"]);
        assert_eq!(records[0].workdir, Some(path("/sandbox/dir")?));
        Ok(())
    })
    .await
}
