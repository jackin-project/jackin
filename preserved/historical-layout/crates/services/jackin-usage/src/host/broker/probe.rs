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

use std::time::Duration;

use jackin_protocol::usage_broker::UsageCoordinationErrorKind;

use crate::coordinator::ProviderProbeOutcome;

/// Budget expiry marker for [`run_probe_with_budget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeBudgetExpired;

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
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let worker =
        jackin_telemetry::spawn::thread_joined_named("usage-broker-probe".to_owned(), move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
            let _ignored = sender.send(outcome);
        });
    let Ok(_worker) = worker else {
        return Err(ProbeBudgetExpired);
    };
    // Dropping the handle detaches the worker; a late send fails silently.
    match receiver.recv_timeout(budget) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => Err(ProbeBudgetExpired),
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
