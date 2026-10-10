// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Worker loop and teardown.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

use std::time::Instant;

use jackin_protocol::usage_broker::UsageCoordinationErrorKind;

use super::{
    ProbeJob, ProviderProbeOutcome, Shared, UsageCoordinator, WorkerMessage, data_bearing,
    finish_failure, finish_success, mark_updating,
};

impl Drop for UsageCoordinator {
    fn drop(&mut self) {
        for _ in &self.workers {
            drop(self.jobs.send(WorkerMessage::Shutdown));
        }
        for worker in self.workers.drain(..) {
            drop(worker.join());
        }
    }
}

pub(crate) fn coordinator_worker(
    shared: &Arc<Shared>,
    receiver: &Arc<Mutex<Receiver<WorkerMessage>>>,
) {
    loop {
        let message = {
            let Ok(receiver) = receiver.lock() else {
                return;
            };
            receiver.recv()
        };
        match message {
            Ok(WorkerMessage::Probe(job)) => execute_probe(shared, job),
            Ok(WorkerMessage::Shutdown) | Err(_) => return,
        }
    }
}

pub(crate) fn execute_probe(shared: &Arc<Shared>, job: ProbeJob) {
    let Some(provider_invoked_at_epoch) = mark_updating(shared, &job) else {
        return;
    };
    let started = Instant::now();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if let Some(scope) = job.credential_scope.as_ref() {
            shared
                .executor
                .probe_scoped(&job.capability, job.generation, scope)
        } else {
            shared.executor.probe(&job.capability, job.generation)
        }
    }));
    let provider_elapsed = started.elapsed();
    let finished_at_epoch = provider_invoked_at_epoch
        .saturating_add(i64::try_from(provider_elapsed.as_secs()).unwrap_or(i64::MAX));
    if provider_elapsed > shared.config.provider_timeout {
        finish_failure(
            shared,
            &job,
            UsageCoordinationErrorKind::ProviderTimeout,
            "usage provider probe timed out",
            None,
            finished_at_epoch,
        );
        return;
    }
    match outcome {
        Ok(ProviderProbeOutcome::Success(view)) if data_bearing(&view) => {
            finish_success(shared, &job, *view, finished_at_epoch);
        }
        Ok(ProviderProbeOutcome::Success(_)) => finish_failure(
            shared,
            &job,
            UsageCoordinationErrorKind::ProviderUnavailable,
            "usage provider returned no quota data",
            None,
            finished_at_epoch,
        ),
        Ok(ProviderProbeOutcome::Failure {
            kind,
            message,
            retry_at_epoch,
        }) => finish_failure(
            shared,
            &job,
            kind,
            &message,
            retry_at_epoch,
            finished_at_epoch,
        ),
        Err(_) => finish_failure(
            shared,
            &job,
            UsageCoordinationErrorKind::OwnerLost,
            "usage provider worker failed",
            None,
            finished_at_epoch,
        ),
    }
}
