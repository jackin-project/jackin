// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use jackin_testkit::TestResult;

const BOUND: Duration = Duration::from_secs(4);

#[test]
fn drive_returns_while_a_blocking_task_is_stuck() -> TestResult {
    let (release, parked) = mpsc::channel::<()>();
    let (started_tx, started_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel::<u8>();
    let driver = std::thread::spawn(move || {
        let output = jackin::drive(async move {
            let stuck = tokio::task::spawn_blocking(move || {
                started_tx.send(()).is_ok() && parked.recv().is_err()
            });
            let started = started_rx.recv().is_ok();
            let abandoned = tokio::time::timeout(Duration::from_millis(50), stuck)
                .await
                .is_err();
            u8::from(started && abandoned)
        });
        done_tx
            .send(output.unwrap_or(u8::MAX))
            .map_err(|_| "the test stopped listening")
    });
    let outcome = done_rx.recv_timeout(BOUND);
    drop(release);
    driver.join().map_err(|_| "driver thread panicked")??;
    assert_eq!(outcome, Ok(1), "drive waited for a stuck blocking task");
    Ok(())
}

#[test]
fn drive_lets_short_blocking_work_finish_before_it_returns() -> TestResult {
    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    jackin::drive(async move {
        drop(tokio::task::spawn_blocking(move || {
            let (_hold, wait) = mpsc::channel::<()>();
            let waited = wait.recv_timeout(Duration::from_millis(300)).is_err();
            flag.store(waited, Ordering::SeqCst);
        }));
        tokio::task::yield_now().await;
    })?;
    assert!(
        finished.load(Ordering::SeqCst),
        "drive abandoned in-flight work"
    );
    Ok(())
}
