// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, Mutex};
use std::time::Duration;

use jackin_protocol::usage_broker::UsageCoordinationErrorKind;

use jackin_usage_coordinator::ProviderProbeOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeBudgetExpired;

#[derive(Clone, Debug)]
pub struct ProbeLiveness {
    active: Arc<Mutex<bool>>,
}

impl ProbeLiveness {
    fn new() -> Self {
        Self {
            active: Arc::new(Mutex::new(true)),
        }
    }

    fn is_current(&self) -> bool {
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn admit_operation(&self) -> bool {
        self.is_current()
    }

    pub fn commit_if_current(&self, operation: &mut dyn FnMut() -> bool) -> Option<bool> {
        let active = self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *active { Some(operation()) } else { None }
    }

    fn close(&self) {
        *self
            .active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = false;
    }
}

pub fn run_probe_with_budget<R, F>(budget: Duration, task: F) -> Result<R, ProbeBudgetExpired>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    run_probe_with_liveness(budget, move |_| task())
}

pub fn run_probe_with_liveness<R, F>(budget: Duration, task: F) -> Result<R, ProbeBudgetExpired>
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
    match receiver.recv_timeout(budget) {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(_) => {
            liveness.close();
            Err(ProbeBudgetExpired)
        }
    }
}

pub fn probe_timeout_outcome() -> ProviderProbeOutcome {
    ProviderProbeOutcome::Failure {
        kind: UsageCoordinationErrorKind::ProviderTimeout,
        message: "usage provider probe timed out".to_owned(),
        retry_at_epoch: None,
    }
}

#[cfg(test)]
mod tests;
