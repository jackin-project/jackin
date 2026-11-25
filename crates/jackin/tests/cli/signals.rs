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

#[test]
fn real_signals_are_handled_one_scenario_at_a_time() -> TestResult {
    super::drive::real_interrupts_reach_a_drive()?;
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(guard(async {
            for (first, second) in [
                ("INT", "INT"),
                ("TERM", "TERM"),
                ("HUP", "HUP"),
                ("HUP", "TERM"),
            ] {
                first_cancels_second_forces_exit(first, second).await?;
            }
            Ok(())
        }))
}
