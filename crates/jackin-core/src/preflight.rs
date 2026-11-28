// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::boundary::Hold;

const PREFLIGHT_SLOTS: usize = 4;

pub(crate) type Slot = Arc<OwnedSemaphorePermit>;

pub(crate) struct TimedOut;

fn preflight_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(PREFLIGHT_SLOTS)))
}

pub(crate) async fn preflight_slot() -> Option<Slot> {
    Arc::clone(preflight_slots())
        .acquire_owned()
        .await
        .ok()
        .map(Arc::new)
}

pub(crate) async fn within_preflight<T, Work, Running>(
    deadline: Duration,
    work: Work,
) -> Result<T, TimedOut>
where
    Work: FnOnce(Hold) -> Running,
    Running: Future<Output = T>,
{
    tokio::time::timeout(deadline, async {
        let slot = preflight_slot().await.ok_or(TimedOut)?;
        let hold: Hold = slot;
        Ok(work(hold).await)
    })
    .await
    .map_err(|_| TimedOut)?
}
