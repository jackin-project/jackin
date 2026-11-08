// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use jackin_core::{ProcessError, ProcessRunner, ProcessStatus};
use jackin_session::TokioProcessRunner;
use tokio_util::sync::CancellationToken;

use crate::support::{
    LOOP_DETACHED, LOOP_HOLDING, Scratch, TestResult, assert_no_leftover, guard, wait_for_beat,
};

#[tokio::test]
async fn run_secret_returns_stdout_only_as_secret_bytes_and_obeys_timeout_and_cancel() -> TestResult
{
    guard(async {
        let scratch = Scratch::new()?;
        let cancel = CancellationToken::new();
        let spec = scratch.spec("printf 'S3cr3t-Value-91\\n'\necho diag >&2\n", &[])?;
        let output = TokioProcessRunner.run_secret(&spec, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert_eq!(output.stdout.expose(), b"S3cr3t-Value-91\n");
        assert_eq!(output.stderr, b"diag\n");
        let shown = format!("{output:?}");
        assert!(shown.contains("<redacted>"), "{shown}");
        assert!(!shown.contains("S3cr3t-Value-91"), "{shown}");
        assert!(!shown.contains("diag"), "{shown}");
        assert!(shown.contains("stderr_len: 5"), "{shown}");

        let failing = scratch.spec("echo 'op: item not found' >&2\nexit 1\n", &[])?;
        let output = TokioProcessRunner.run_secret(&failing, &cancel).await?;
        assert_eq!(output.status, ProcessStatus::Exited(1));
        assert!(output.stdout.expose().is_empty());
        assert_eq!(output.stderr, b"op: item not found\n");

        let timed = Scratch::new()?;
        let mut spec = timed.spec(&format!("{LOOP_DETACHED}echo ready\nwait\n"), &[])?;
        spec.timeout = Duration::from_secs(3);
        match TokioProcessRunner.run_secret(&spec, &cancel).await {
            Err(ProcessError::Timeout { after, .. }) => assert_eq!(after, Duration::from_secs(3)),
            other => return Err(format!("expected Timeout, got {other:?}").into()),
        }
        assert_no_leftover(timed.root()).await?;

        let stopped = Scratch::new()?;
        let spec = stopped.spec(&format!("{LOOP_HOLDING}echo ready\nwait\n"), &[])?;
        let token = CancellationToken::new();
        let run = TokioProcessRunner.run_secret(&spec, &token);
        let driver = async {
            wait_for_beat(stopped.root()).await?;
            token.cancel();
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (result, driven) = tokio::join!(run, driver);
        driven?;
        match result {
            Err(ProcessError::Cancelled { .. }) => {}
            other => return Err(format!("expected Cancelled, got {other:?}").into()),
        }
        assert_no_leftover(stopped.root()).await
    })
    .await
}
