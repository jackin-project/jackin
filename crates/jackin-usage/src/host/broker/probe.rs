// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded host-side budgets for synchronous provider probes.
//!
//! The coordinator only classifies elapsed time *after* a probe returns, so an
//! unbounded adapter call (a hung child process or RPC) would hold a
//! coordinator worker forever and stall every account behind it. This module
//! bounds the wait: the blocking probe runs on a worker thread and the broker
//! reclaims its generation when the budget expires.
//!
//! Budget expiry returns a typed [`ProviderProbeOutcome::Failure`]; it never
//! cancels broker state. The coordinator completes the generation through its
//! normal failure path, which preserves last-good quota and keeps generation
//! ownership intact for the next request. A late worker result is dropped:
//! its channel send fails once the broker has moved on.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::usage_broker::UsageCoordinationErrorKind;

use crate::coordinator::ProviderProbeOutcome;

/// Budget expiry marker for [`run_probe_with_budget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeBudgetExpired;

/// Per-probe authorization fence shared with a detached task. A budget
/// timeout closes it before the coordinator terminalizes the generation, so
/// late Claude work can finish an already admitted operation but cannot begin
/// another request or commit refreshed credential material.
#[derive(Clone)]
pub(crate) struct ProbeLiveness {
    active: Arc<Mutex<bool>>,
}

/// Admission token for one bounded provider or Keychain operation. Acquiring
/// it under `active` is the operation's ordering point against timeout; the
/// external call runs without holding the mutex, so timeout never waits for
/// blocked I/O. An operation admitted before close may finish afterward.
pub(crate) struct ProbeOperationPermit {
    _active: Arc<Mutex<bool>>,
}

impl ProbeLiveness {
    fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(true)),
        }
    }

    /// Check whether this probe may begin another operation.
    pub(crate) fn is_current(&self) -> bool {
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Atomically admit one operation unless this probe has timed out or
    /// completed. Keep the returned token alive for the complete external
    /// operation; every later operation must acquire a new token.
    pub(crate) fn admit_operation(&self) -> Option<ProbeOperationPermit> {
        let active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (*active).then(|| ProbeOperationPermit {
            _active: Arc::clone(&self.active),
        })
    }

    /// Run one short commit while holding the cancellation gate. If timeout
    /// wins the gate first, the commit is rejected; if the commit wins first,
    /// timeout waits until it is complete before closing the probe.
    pub(crate) fn commit_if_current(&self, operation: &mut dyn FnMut() -> bool) -> Option<bool> {
        let active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (*active).then(operation)
    }

    fn close(&self) {
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
    }
}

/// Run a blocking provider probe to completion or budget expiry.
///
/// A worker panic propagates to the caller (the coordinator classifies it as
/// `OwnerLost`, exactly as if the probe had run inline). On expiry the worker
/// is detached; adapter-level timeouts bound its remaining lifetime and its
/// late result is dropped instead of being merged.
pub(crate) fn run_probe_with_budget<R, F>(
    budget: Duration,
    task: F,
) -> Result<R, ProbeBudgetExpired>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    run_probe_with_liveness(budget, move |_| task())
}

/// Variant of [`run_probe_with_budget`] that gives the worker its revocable
/// admission fence. Non-Claude probes keep the simpler no-argument API.
pub(crate) fn run_probe_with_liveness<R, F>(
    budget: Duration,
    task: F,
) -> Result<R, ProbeBudgetExpired>
where
    F: FnOnce(ProbeLiveness) -> R + Send + 'static,
    R: Send + 'static,
{
    let liveness = ProbeLiveness::new();
    let worker_liveness = liveness.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker =
        jackin_telemetry::spawn::thread_joined_named("usage-broker-probe".to_owned(), move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                task(worker_liveness.clone())
            }));
            worker_liveness.close();
            let _ignored = sender.send(outcome);
        });
    let Ok(_worker) = worker else {
        return Err(ProbeBudgetExpired);
    };
    // Dropping the handle detaches the worker; a late send fails silently.
    match receiver.recv_timeout(budget) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => {
            liveness.close();
            Err(ProbeBudgetExpired)
        }
    }
}

/// Typed probe failure produced when a provider probe exceeds its budget.
///
/// The coordinator maps this to a `Failed` generation that keeps last-good
/// quota and its normal retry lifecycle; broker ownership is unaffected.
pub(crate) fn probe_timeout_outcome() -> ProviderProbeOutcome {
    ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::ProviderTimeout,
        message: "usage provider probe timed out".to_owned(),
        retry_at_epoch: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_closes_probe_before_detached_worker_resumes() {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (late_result_tx, late_result_rx) = std::sync::mpsc::channel();

        let result = run_probe_with_liveness(Duration::from_millis(50), move |liveness| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            late_result_tx
                .send((liveness.is_current(), liveness.admit_operation().is_some()))
                .unwrap();
        });

        assert_eq!(result, Err(ProbeBudgetExpired));
        started_rx
            .recv()
            .expect("detached worker started before its budget expired");
        release_tx.send(()).unwrap();
        assert_eq!(
            late_result_rx.recv().unwrap(),
            (false, false),
            "timeout must revoke admission before the late worker resumes"
        );
    }

    #[test]
    fn close_rejects_new_operation_admission_but_keeps_prior_token_alive() {
        let liveness = ProbeLiveness::new();
        let admitted = liveness
            .admit_operation()
            .expect("active probe admits its first operation");

        liveness.close();

        assert!(!liveness.is_current());
        assert!(liveness.admit_operation().is_none());
        drop(admitted);
    }
}
