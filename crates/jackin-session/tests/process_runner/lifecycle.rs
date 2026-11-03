// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::{Duration, Instant};

use jackin_core::{
    HostPath, OutputLine, OutputStream, ProcessError, ProcessRunner, ProcessStatus, Program,
};
use jackin_session::TokioProcessRunner;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{
    GROUP_ID, LOOP_DETACHED, LOOP_HOLDING, Scratch, TestResult, WAIT_FOR, assert_no_leftover,
    guarded, next_line, wait_for_beat,
};

#[tokio::test]
async fn a_timeout_kills_the_child_and_every_descendant_and_reports_the_configured_limit()
-> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let mut spec = scratch.spec(&format!("{LOOP_DETACHED}echo ready\nwait\n"), &[])?;
        spec.timeout = Duration::from_secs(3);
        let (tx, mut rx) = mpsc::channel::<OutputLine>(8);
        let cancel = CancellationToken::new();
        let started = Instant::now();
        let run = async {
            let result = TokioProcessRunner.run(&spec, Some(&tx), &cancel).await;
            drop(tx);
            result
        };
        let collect = async {
            let mut seen = Vec::new();
            while let Some(line) = rx.recv().await {
                seen.push((line.stream, line.text));
            }
            seen
        };
        let (result, seen) = tokio::join!(run, collect);
        let elapsed = started.elapsed();
        match result {
            Err(ProcessError::Timeout {
                program,
                after,
                group: None,
            }) => {
                assert_eq!(program, Program::Git);
                assert_eq!(after, Duration::from_secs(3), "the configured limit");
            }
            other => return Err(format!("expected Timeout, got {other:?}").into()),
        }
        assert!(
            elapsed >= Duration::from_secs(3),
            "returned after {elapsed:?}"
        );
        assert!(
            seen.contains(&(OutputStream::Stdout, "ready".to_owned())),
            "the child never ran: {seen:?}"
        );
        assert_no_leftover(scratch.root()).await?;

        let holding = Scratch::new()?;
        let spec = holding.spec(&format!("{LOOP_HOLDING}echo ready\n"), &[])?;
        let started = Instant::now();
        let output = TokioProcessRunner
            .run(&spec, None, &CancellationToken::new())
            .await?;
        let elapsed = started.elapsed();
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert!(
            elapsed >= Duration::from_secs(2) && elapsed < Duration::from_secs(6),
            "a pipe-holding descendant ended the call after {elapsed:?}"
        );
        assert_no_leftover(holding.root()).await
    })
    .await
}

#[tokio::test]
async fn cancel_kills_the_child_and_every_descendant_and_reports_cancelled() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let body = format!("{LOOP_HOLDING}echo ready\nwait\n");
        let mut spec = scratch.spec(&body, &[])?;
        spec.timeout = Duration::from_secs(120);
        let (tx, mut rx) = mpsc::channel::<OutputLine>(8);
        let cancel = CancellationToken::new();
        let run = TokioProcessRunner.run(&spec, Some(&tx), &cancel);
        let driver = async {
            let line = next_line(&mut rx, "the ready line before the cancel").await?;
            assert_eq!(line.text, "ready");
            wait_for_beat(scratch.root()).await?;
            cancel.cancel();
            Ok::<_, Box<dyn std::error::Error>>(())
        };
        let (result, driven) = tokio::join!(run, driver);
        driven?;
        match result {
            Err(ProcessError::Cancelled {
                program,
                group: None,
            }) => assert_eq!(program, Program::Git),
            other => return Err(format!("expected Cancelled, got {other:?}").into()),
        }
        assert_no_leftover(scratch.root()).await?;

        let before = Scratch::new()?;
        let mut spec = before.spec("echo unreachable\n", &[])?;
        spec.cwd = Some(HostPath::parse(&before.root().join("no-such-dir"))?);
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        match TokioProcessRunner.run(&spec, None, &cancelled).await {
            Err(ProcessError::Cancelled {
                program,
                group: None,
            }) => assert_eq!(program, Program::Git),
            other => return Err(format!("expected Cancelled, got {other:?}").into()),
        }
        match TokioProcessRunner
            .run(&spec, None, &CancellationToken::new())
            .await
        {
            Err(ProcessError::Spawn { program, .. }) => assert_eq!(program, Program::Git),
            other => return Err(format!("expected Spawn, got {other:?}").into()),
        }

        let dropped = Scratch::new()?;
        let spec = dropped.spec(&format!("{LOOP_HOLDING}echo ready\nwait\n"), &[])?;
        let (tx, mut rx) = mpsc::channel::<OutputLine>(8);
        let cancel = CancellationToken::new();
        let run = TokioProcessRunner.run(&spec, Some(&tx), &cancel);
        let ready = async {
            next_line(&mut rx, "the ready line before the drop").await?;
            wait_for_beat(dropped.root()).await
        };
        tokio::select! {
            result = run => return Err(format!("the run ended on its own: {result:?}").into()),
            ready = ready => ready?,
        }
        assert_no_leftover(dropped.root()).await
    })
    .await
}

#[tokio::test]
async fn a_clean_exit_kills_a_descendant_that_closed_its_pipes() -> TestResult {
    guarded(async {
        let body = format!("{WAIT_FOR}{LOOP_DETACHED}wait_for \"$d/beat\"\nsleep 0.2\nexit 0\n");
        let plain = Scratch::new()?;
        let output = TokioProcessRunner
            .run(&plain.spec(&body, &[])?, None, &CancellationToken::new())
            .await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert_no_leftover(plain.root()).await?;

        let secret = Scratch::new()?;
        let output = TokioProcessRunner
            .run_secret(&secret.spec(&body, &[])?, &CancellationToken::new())
            .await?;
        assert_eq!(output.status, ProcessStatus::Exited(0));
        assert_no_leftover(secret.root()).await?;

        let failing = Scratch::new()?;
        let body = body.replace("exit 0", "exit 7");
        let output = TokioProcessRunner
            .run(&failing.spec(&body, &[])?, None, &CancellationToken::new())
            .await?;
        assert_eq!(output.status, ProcessStatus::Exited(7));
        assert_no_leftover(failing.root()).await
    })
    .await
}

const GROUP_BODY: &str =
    "d=$1\nfor n in 1 2 3 4 5 6 7 8; do ( while :; do sleep 0.05; done ) >/dev/null 2>&1 & done\n";

fn group_is_empty(scratch: &Scratch) -> Result<bool, Box<dyn std::error::Error>> {
    let raw: i32 = std::fs::read_to_string(scratch.root().join("pgid"))?
        .trim()
        .parse()?;
    let group = rustix::process::Pid::from_raw(raw).ok_or("the group id is not a pid")?;
    Ok(rustix::process::test_kill_process_group(group).is_err())
}

async fn exit_round(round: usize) -> TestResult {
    let scratch = Scratch::new()?;
    let body = format!("{GROUP_BODY}{GROUP_ID}exit 0\n");
    let output = TokioProcessRunner
        .run(&scratch.spec(&body, &[])?, None, &CancellationToken::new())
        .await?;
    assert_eq!(output.status, ProcessStatus::Exited(0));
    assert!(group_is_empty(&scratch)?, "exit, round {round}");
    Ok(())
}

async fn cancel_once_the_group_is_known(scratch: &Scratch, cancel: &CancellationToken) {
    while !scratch.root().join("pgid").exists() {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    cancel.cancel();
}

async fn cancel_round(round: usize) -> TestResult {
    let scratch = Scratch::new()?;
    let body = format!("{GROUP_BODY}{GROUP_ID}sleep 30\n");
    let spec = scratch.spec(&body, &[])?;
    let cancel = CancellationToken::new();
    let (result, ()) = tokio::join!(
        TokioProcessRunner.run(&spec, None, &cancel),
        cancel_once_the_group_is_known(&scratch, &cancel)
    );
    assert!(
        matches!(result, Err(ProcessError::Cancelled { .. })),
        "{result:?}"
    );
    assert!(group_is_empty(&scratch)?, "cancel, round {round}");
    Ok(())
}

#[tokio::test]
async fn a_run_returns_only_after_no_process_of_its_group_is_left() -> TestResult {
    guarded(async {
        for round in 0..40 {
            exit_round(round).await?;
        }
        for round in 0..40 {
            cancel_round(round).await?;
        }
        Ok(())
    })
    .await
}
