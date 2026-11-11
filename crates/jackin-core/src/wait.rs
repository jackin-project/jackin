// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::boundary::{Clock, FileLock, FileSystem, FsError};
use crate::display::plain_duration;
use crate::paths::HostPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollPlan {
    pub interval: Duration,
    pub timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll<T> {
    Ready(T),
    Pending,
}

#[derive(Debug, Error)]
pub enum PollError<E: std::error::Error + 'static> {
    #[error("timed out after {}", plain_duration(.after))]
    Timeout { after: Duration },
    #[error("cancelled")]
    Cancelled,
    #[error("probe failed")]
    Probe(#[source] E),
}

const MIN_INTERVAL: Duration = Duration::from_millis(1);

pub async fn poll_until<T, E, Fut, F>(
    clock: &dyn Clock,
    plan: PollPlan,
    cancel: &CancellationToken,
    mut probe: F,
) -> Result<T, PollError<E>>
where
    E: std::error::Error + 'static,
    F: FnMut() -> Fut + Send,
    Fut: Future<Output = Result<Poll<T>, E>> + Send,
{
    let interval = plan.interval.max(MIN_INTERVAL);
    let mut elapsed = Duration::ZERO;
    loop {
        let remaining = plan.timeout.saturating_sub(elapsed);
        let outcome = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(PollError::Cancelled),
            outcome = probe() => outcome,
            () = clock.sleep(remaining) => {
                return Err(PollError::Timeout { after: plan.timeout });
            }
        };
        match outcome.map_err(PollError::Probe)? {
            Poll::Ready(value) => return Ok(value),
            Poll::Pending => {}
        }
        if cancel.is_cancelled() {
            return Err(PollError::Cancelled);
        }
        if remaining.is_zero() {
            return Err(PollError::Timeout { after: elapsed });
        }
        let step = interval.min(remaining);
        if cancel
            .run_until_cancelled(clock.sleep(step))
            .await
            .is_none()
        {
            return Err(PollError::Cancelled);
        }
        elapsed += step;
    }
}

pub async fn hold_lock(
    fs: &dyn FileSystem,
    clock: &dyn Clock,
    path: &HostPath,
    plan: PollPlan,
    cancel: &CancellationToken,
) -> Result<FileLock, PollError<FsError>> {
    poll_until(clock, plan, cancel, || async {
        match fs.lock(path).await {
            Ok(lock) => Ok(Poll::Ready(lock)),
            Err(FsError::LockHeld(_)) => Ok(Poll::Pending),
            Err(error) => Err(error),
        }
    })
    .await
}
