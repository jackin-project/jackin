// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::process::Command;
use std::time::Duration;

use jackin::Signals;
use jackin_testkit::{TestResult, guard};
use tokio_util::sync::CancellationToken;

const WAIT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(100);

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

async fn first_cancels_second_abandons(first: &str, second: &str) -> TestResult {
    let signals = Signals::install()?;
    let cancel = CancellationToken::new();
    let abandon = CancellationToken::new();
    let run = signals.interrupted(&cancel, &abandon);
    tokio::pin!(run);

    send(first)?;
    tokio::select! {
        biased;
        cancelled = tokio::time::timeout(WAIT, cancel.cancelled()) => {
            assert!(cancelled.is_ok(), "{first} did not cancel the run");
        }
        never = &mut run => match never {},
    }
    assert!(
        !abandon.is_cancelled(),
        "{first} abandoned the run on its own"
    );

    send(second)?;
    tokio::select! {
        biased;
        abandoned = tokio::time::timeout(WAIT, abandon.cancelled()) => {
            assert!(abandoned.is_ok(), "{second} did not abandon the run after {first}");
        }
        never = &mut run => match never {},
    }
    assert!(
        tokio::time::timeout(SETTLE, &mut run).await.is_err(),
        "the signal task must stay pending after the second signal"
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
                first_cancels_second_abandons(first, second).await?;
            }
            Ok(())
        }))
}
