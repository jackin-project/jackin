// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{Clock, Poll, PollError, PollPlan, Timestamp, poll_until};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Debug, Error, PartialEq)]
enum ProbeError {
    #[error("runaway probe")]
    Runaway,
    #[error("boom")]
    Boom,
}

#[derive(Debug, Default)]
struct VirtualClock {
    elapsed: Mutex<Duration>,
}

impl VirtualClock {
    fn elapsed(&self) -> Duration {
        *self.elapsed.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl Clock for VirtualClock {
    fn now(&self) -> Timestamp {
        Timestamp(i64::try_from(self.elapsed().as_secs()).unwrap_or(i64::MAX))
    }

    fn utc_offset_secs(&self) -> i32 {
        0
    }

    async fn sleep(&self, duration: Duration) {
        *self.elapsed.lock().unwrap_or_else(PoisonError::into_inner) += duration;
    }
}

fn plan(interval: u64, timeout: u64) -> PollPlan {
    PollPlan {
        interval: Duration::from_secs(interval),
        timeout: Duration::from_secs(timeout),
    }
}

fn guard(calls: &AtomicUsize) -> Result<usize, ProbeError> {
    let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
    if call > 1000 {
        Err(ProbeError::Runaway)
    } else {
        Ok(call)
    }
}

#[tokio::test]
async fn poll_until_returns_the_ready_value_after_exactly_the_waits_it_needed() -> TestResult {
    let clock = VirtualClock::default();
    let calls = AtomicUsize::new(0);
    let result: Result<&str, PollError<ProbeError>> =
        poll_until(&clock, plan(5, 60), &CancellationToken::new(), || {
            let call = guard(&calls);
            async move {
                Ok(if call? == 3 {
                    Poll::Ready("done")
                } else {
                    Poll::Pending
                })
            }
        })
        .await;
    assert_eq!(result?, "done");
    assert_eq!(clock.elapsed(), Duration::from_secs(10));
    assert_eq!(calls.load(Ordering::SeqCst), 3);

    let clock = VirtualClock::default();
    let result: Result<&str, PollError<ProbeError>> =
        poll_until(&clock, plan(5, 60), &CancellationToken::new(), || async {
            Ok(Poll::Ready("now"))
        })
        .await;
    assert_eq!(result?, "now");
    assert_eq!(clock.elapsed(), Duration::ZERO);
    Ok(())
}

#[tokio::test]
async fn poll_until_times_out_after_exactly_the_timeout() -> TestResult {
    let clock = VirtualClock::default();
    let calls = AtomicUsize::new(0);
    let result: Result<(), PollError<ProbeError>> =
        poll_until(&clock, plan(3, 10), &CancellationToken::new(), || {
            let call = guard(&calls);
            async move { call.map(|_| Poll::Pending) }
        })
        .await;
    match result {
        Err(PollError::Timeout { after }) => assert_eq!(after, Duration::from_secs(10)),
        other => panic!("{other:?}"),
    }
    assert!(clock.elapsed() <= Duration::from_secs(10));
    assert!(clock.elapsed() >= Duration::from_secs(10) - Duration::from_millis(1));
    Ok(())
}

#[derive(Debug, Default)]
struct RealClock;

#[async_trait]
impl Clock for RealClock {
    fn now(&self) -> Timestamp {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        Timestamp(i64::try_from(seconds).unwrap_or(i64::MAX))
    }

    fn utc_offset_secs(&self) -> i32 {
        0
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[tokio::test]
async fn poll_until_counts_the_time_a_pending_probe_spends() -> TestResult {
    let clock = RealClock;
    let calls = AtomicUsize::new(0);
    let result: Result<(), PollError<ProbeError>> = poll_until(
        &clock,
        PollPlan {
            interval: Duration::from_millis(10),
            timeout: Duration::from_millis(100),
        },
        &CancellationToken::new(),
        || {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                tokio::time::sleep(Duration::from_millis(60)).await;
                Ok(Poll::Pending)
            }
        },
    )
    .await;
    match result {
        Err(PollError::Timeout { after }) => {
            assert_eq!(after, Duration::from_millis(100));
        }
        other => panic!("{other:?}"),
    }
    assert!(calls.load(Ordering::SeqCst) <= 3, "the probe ran too often");
    Ok(())
}

#[tokio::test]
async fn poll_until_subtracts_probe_time_from_the_inter_probe_sleep() -> TestResult {
    #[derive(Debug, Default)]
    struct RecordingClock {
        slept: Mutex<Vec<Duration>>,
    }

    #[async_trait]
    impl Clock for RecordingClock {
        fn now(&self) -> Timestamp {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs());
            Timestamp(i64::try_from(seconds).unwrap_or(i64::MAX))
        }

        fn utc_offset_secs(&self) -> i32 {
            0
        }

        async fn sleep(&self, duration: Duration) {
            tokio::time::sleep(duration).await;
            self.slept
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(duration);
        }
    }

    let clock = RecordingClock::default();
    let calls = AtomicUsize::new(0);
    let result: Result<(), PollError<ProbeError>> = poll_until(
        &clock,
        PollPlan {
            interval: Duration::from_millis(30),
            timeout: Duration::from_millis(100),
        },
        &CancellationToken::new(),
        || {
            calls.fetch_add(1, Ordering::SeqCst);
            async move {
                tokio::time::sleep(Duration::from_millis(25)).await;
                Ok(Poll::Pending)
            }
        },
    )
    .await;
    match result {
        Err(PollError::Timeout { after }) => assert_eq!(after, Duration::from_millis(100)),
        other => panic!("{other:?}"),
    }
    let slept: Duration = clock
        .slept
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .sum();
    assert!(
        slept <= Duration::from_millis(55),
        "the inter-probe sleeps exceeded the remaining budget: {slept:?}"
    );
    Ok(())
}

#[tokio::test]
async fn poll_until_returns_a_probe_error_at_once() -> TestResult {
    let clock = VirtualClock::default();
    let calls = AtomicUsize::new(0);
    let result: Result<(), PollError<ProbeError>> =
        poll_until(&clock, plan(5, 60), &CancellationToken::new(), || {
            let call = guard(&calls);
            async move {
                match call? {
                    2 => Err(ProbeError::Boom),
                    _ => Ok(Poll::Pending),
                }
            }
        })
        .await;
    assert!(
        matches!(result, Err(PollError::Probe(ProbeError::Boom))),
        "{result:?}"
    );
    assert_eq!(clock.elapsed(), Duration::from_secs(5));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    Ok(())
}

#[tokio::test]
async fn poll_until_stops_after_cancellation() -> TestResult {
    let clock = VirtualClock::default();
    let calls = AtomicUsize::new(0);
    let cancel = CancellationToken::new();
    let result: Result<(), PollError<ProbeError>> =
        poll_until(&clock, plan(5, 60), &cancel, || {
            let call = guard(&calls);
            cancel.cancel();
            async move { call.map(|_| Poll::Pending) }
        })
        .await;
    assert!(matches!(result, Err(PollError::Cancelled)), "{result:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(clock.elapsed() <= Duration::from_secs(5));
    Ok(())
}

#[derive(Debug)]
struct GateClock {
    slept: Mutex<Vec<Duration>>,
    permits: Semaphore,
}

impl GateClock {
    fn new() -> GateClock {
        GateClock {
            slept: Mutex::new(Vec::new()),
            permits: Semaphore::new(0),
        }
    }

    fn sleeps(&self) -> Vec<Duration> {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl Clock for GateClock {
    fn now(&self) -> Timestamp {
        Timestamp(0)
    }

    fn utc_offset_secs(&self) -> i32 {
        0
    }

    async fn sleep(&self, duration: Duration) {
        self.slept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(duration);
        if let Ok(permit) = self.permits.acquire().await {
            permit.forget();
        }
    }
}

const SETTLE_ROUNDS: usize = 10_000;

async fn until(what: &str, mut ready: impl FnMut() -> bool) -> TestResult {
    for _ in 0..SETTLE_ROUNDS {
        if ready() {
            return Ok(());
        }
        tokio::task::yield_now().await;
    }
    Err(what.into())
}

#[tokio::test]
async fn poll_until_cuts_a_probe_that_never_returns_at_the_timeout() -> TestResult {
    let clock = Arc::new(GateClock::new());
    let task_clock = Arc::clone(&clock);
    let task = tokio::spawn(async move {
        let result: Result<(), PollError<ProbeError>> = poll_until(
            task_clock.as_ref(),
            plan(5, 10),
            &CancellationToken::new(),
            std::future::pending::<Result<Poll<()>, ProbeError>>,
        )
        .await;
        result
    });
    until("no sleep was started while the probe was pending", || {
        !clock.sleeps().is_empty()
    })
    .await?;
    for slept in clock.sleeps() {
        assert!(
            slept > Duration::ZERO && slept <= Duration::from_secs(10),
            "{slept:?}"
        );
    }
    assert!(!task.is_finished());
    clock.permits.add_permits(1000);
    until("task did not finish", || task.is_finished()).await?;
    match task.await? {
        Err(PollError::Timeout { after }) => assert_eq!(after, Duration::from_secs(10)),
        other => return Err(format!("{other:?}").into()),
    }
    Ok(())
}

#[derive(Debug)]
struct DropFlag(Arc<AtomicUsize>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn poll_until_stops_a_hanging_probe_when_cancelled() -> TestResult {
    let clock = Arc::new(GateClock::new());
    let task_clock = Arc::clone(&clock);
    let started = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let cancel = CancellationToken::new();
    let (probe_started, probe_dropped, task_cancel) =
        (Arc::clone(&started), Arc::clone(&dropped), cancel.clone());
    let task = tokio::spawn(async move {
        let result: Result<(), PollError<ProbeError>> =
            poll_until(task_clock.as_ref(), plan(5, 60), &task_cancel, || {
                let started = Arc::clone(&probe_started);
                let guard = DropFlag(Arc::clone(&probe_dropped));
                async move {
                    started.fetch_add(1, Ordering::SeqCst);
                    let _guard = guard;
                    std::future::pending::<Result<Poll<()>, ProbeError>>().await
                }
            })
            .await;
        result
    });
    until("the probe never started", || {
        started.load(Ordering::SeqCst) == 1
    })
    .await?;
    cancel.cancel();
    until("task did not finish", || task.is_finished()).await?;
    let result = task.await?;
    assert!(matches!(result, Err(PollError::Cancelled)), "{result:?}");
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    Ok(())
}
