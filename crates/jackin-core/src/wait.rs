// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use thiserror::Error;

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
    #[error("timed out after {after:?}")]
    Timeout { after: Duration },
    #[error("cancelled")]
    Cancelled,
    #[error("probe failed")]
    Probe(#[source] E),
}
