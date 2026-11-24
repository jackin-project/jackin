// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::mpsc;
use std::time::Duration;

use jackin_testkit::TestResult;

const BOUND: Duration = Duration::from_secs(5);

#[test]
fn drive_returns_while_a_blocking_task_is_stuck() -> TestResult {
    let (release, parked) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<u8>();
    let driver = std::thread::spawn(move || {
        let output = jackin::drive(async move {
            let stuck = tokio::task::spawn_blocking(move || {
                drop(started_tx.send(()));
                drop(parked.recv());
            });
            started_rx.recv().ok();
            tokio::time::timeout(Duration::from_millis(50), stuck)
                .await
                .is_err()
                .into()
        });
        drop(done_tx.send(output.unwrap_or(u8::MAX)));
    });
    let outcome = done_rx.recv_timeout(BOUND);
    drop(release);
    driver.join().map_err(|_| "driver thread panicked")?;
    assert_eq!(outcome, Ok(1), "drive waited for a stuck blocking task");
    Ok(())
}
