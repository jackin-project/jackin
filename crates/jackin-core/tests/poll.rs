// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use jackin_core::{Clock, Poll, PollError, PollPlan, Timestamp, poll_until};
use thiserror::Error;
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
    assert_eq!(clock.elapsed(), Duration::from_secs(10));
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
