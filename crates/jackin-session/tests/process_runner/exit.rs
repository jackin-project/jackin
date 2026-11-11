// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{
    EnvValue, HostPath, ProcessError, ProcessRunner, ProcessSpec, ProcessStatus, ProcessStdin,
    Program, Secret, SecretBytes,
};
use jackin_session::TokioProcessRunner;
use tokio_util::sync::CancellationToken;

use crate::support::{Scratch, TestResult, guarded, text};

#[tokio::test]
async fn exit_and_signal_statuses_are_reported_as_what_happened() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let rows = [
            ("exit 0", ProcessStatus::Exited(0)),
            ("exit 7", ProcessStatus::Exited(7)),
            ("exit 137", ProcessStatus::Exited(137)),
            ("kill -KILL \"$PPID\"\nexit 0", ProcessStatus::Signaled(9)),
            ("kill -TERM \"$PPID\"\nexit 0", ProcessStatus::Signaled(15)),
        ];
        for (body, expected) in rows {
            let spec = scratch.spec(body, &[])?;
            let output = TokioProcessRunner
                .run(&spec, None, &CancellationToken::new())
                .await?;
            assert_eq!(output.status, expected, "status of {body:?}");
        }
        Ok(())
    })
    .await
}

fn secret_spec(program: Program, path: &str) -> ProcessSpec {
    ProcessSpec {
        program,
        args: vec!["read".to_owned()],
        cwd: None,
        env: vec![
            ("PATH".to_owned(), EnvValue::Plain(path.to_owned())),
            (
                "JK_TOKEN".to_owned(),
                EnvValue::Secret(Secret::new("S3cr3t-Env-77".to_owned())),
            ),
        ],
        env_remove_prefixes: Vec::new(),
        stdin: ProcessStdin::Bytes(SecretBytes::new(b"S3cr3t-In-88".to_vec())),
        timeout: Duration::from_secs(10),
    }
}

fn assert_spawn_failure(label: &str, error: &ProcessError, program: Program) -> TestResult {
    let ProcessError::Spawn {
        program: found,
        message,
    } = error
    else {
        return Err(format!("{label}: expected Spawn, got {error}").into());
    };
    assert_eq!(*found, program, "{label}: program");
    assert!(!message.is_empty(), "{label}: empty message");
    let shown = format!("{error} {error:?}");
    for secret in ["S3cr3t-Env-77", "S3cr3t-In-88"] {
        assert!(
            !shown.contains(secret),
            "{label}: {secret} leaked in {shown}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_program_or_directory_that_cannot_start_gives_spawn_without_echoing_secrets() -> TestResult
{
    guarded(async {
        let scratch = Scratch::new()?;
        let empty = scratch.root().join("empty-path");
        std::fs::create_dir(&empty)?;
        let spec = secret_spec(Program::Op, &text(&empty)?);
        let cancel = CancellationToken::new();
        let run = TokioProcessRunner.run(&spec, None, &cancel).await;
        let Err(error) = run else {
            return Err("run found a program on an empty PATH".into());
        };
        assert_spawn_failure("run", &error, Program::Op)?;
        let secret = TokioProcessRunner.run_secret(&spec, &cancel).await;
        let Err(error) = secret else {
            return Err("run_secret found a program on an empty PATH".into());
        };
        assert_spawn_failure("run_secret", &error, Program::Op)?;
        let mut missing = scratch.spec("exit 0", &[])?;
        missing.cwd = Some(HostPath::parse(&scratch.root().join("no-such-dir"))?);
        let Err(error) = TokioProcessRunner.run(&missing, None, &cancel).await else {
            return Err("run started a child in a missing directory".into());
        };
        assert_spawn_failure("missing cwd", &error, Program::Git)
    })
    .await
}
