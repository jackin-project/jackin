// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;
use std::time::Duration;

use jackin::Signals;
use jackin_testkit::{TestResult, guard};
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(5);

fn send(signal: &str) -> TestResult {
    let status = Command::new("/bin/kill")
        .args(["-s", signal, &std::process::id().to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill -s {signal} failed: {status}").into())
    }
}

async fn first_cancels_second_forces_exit(first: &str, second: &str) -> TestResult {
    let signals = Signals::install()?;
    let cancel = CancellationToken::new();
    let run = signals.interrupted(&cancel);
    tokio::pin!(run);

    send(first)?;
    let cancelled = tokio::select! {
        biased;
        cancelled = tokio::time::timeout(WAIT, cancel.cancelled()) => cancelled.is_ok(),
        () = &mut run => return Err(format!("{first} ended the wait without a cancel").into()),
    };
    assert!(cancelled, "{first} did not cancel the run");

    send(second)?;
    let forced = tokio::time::timeout(WAIT, &mut run).await;
    assert!(
        forced.is_ok(),
        "{second} did not force the exit after {first}"
    );
    Ok(())
}

#[tokio::test]
async fn sigint_cancels_and_a_second_signal_forces_the_exit() -> TestResult {
    guard(first_cancels_second_forces_exit("INT", "INT")).await
}

#[tokio::test]
async fn sigterm_cancels_like_sigint() -> TestResult {
    guard(first_cancels_second_forces_exit("TERM", "TERM")).await
}

#[tokio::test]
async fn sighup_cancels_like_sigint() -> TestResult {
    guard(first_cancels_second_forces_exit("HUP", "HUP")).await
}

#[tokio::test]
async fn the_signals_count_together_toward_the_forced_exit() -> TestResult {
    guard(first_cancels_second_forces_exit("HUP", "TERM")).await
}
