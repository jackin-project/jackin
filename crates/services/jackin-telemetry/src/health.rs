// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};

use crate::event::Rejection;

const REASONS: usize = 6;
static REJECTIONS: [AtomicU64; 18] = [const { AtomicU64::new(0) }; 18];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum Signal {
    Log,
    Trace,
    Metric,
}

impl Signal {
    pub const ALL: [Self; 3] = [Self::Log, Self::Trace, Self::Metric];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Log => "log",
            Self::Trace => "trace",
            Self::Metric => "metric",
        }
    }
}

pub(crate) fn count(signal: Signal, reason: Rejection) -> u64 {
    REJECTIONS[signal as usize * REASONS + reason as usize].load(Ordering::Relaxed)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FacadeHealth {
    pub unknown_name: u64,
    pub unknown_attribute: u64,
    pub invalid_value: u64,
    pub privacy: u64,
    pub cardinality: u64,
    pub size_limit: u64,
    pub by_signal_reason: [[u64; REASONS]; 3],
}

pub(crate) fn reject(signal: Signal, reason: Rejection) {
    REJECTIONS[signal as usize * REASONS + reason as usize].fetch_add(1, Ordering::Relaxed);
}

#[doc(hidden)]
pub fn record_export_rejection(signal: Signal, reason: Rejection) {
    reject(signal, reason);
}

#[must_use]
pub fn facade_health() -> FacadeHealth {
    let by_signal_reason = std::array::from_fn(|signal| {
        std::array::from_fn(|reason| REJECTIONS[signal * REASONS + reason].load(Ordering::Relaxed))
    });
    let total = |reason: usize| by_signal_reason.iter().map(|row| row[reason]).sum();
    FacadeHealth {
        unknown_name: total(0),
        unknown_attribute: total(1),
        invalid_value: total(2),
        privacy: total(3),
        cardinality: total(4),
        size_limit: total(5),
        by_signal_reason,
    }
}
