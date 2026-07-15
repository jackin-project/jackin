// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering};

use crate::event::Rejection;

static REJECTIONS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FacadeHealth {
    pub unknown_name: u64,
    pub unknown_attribute: u64,
    pub invalid_value: u64,
    pub privacy: u64,
    pub cardinality: u64,
    pub size_limit: u64,
}

pub(crate) fn reject(reason: Rejection) {
    REJECTIONS[reason as usize].fetch_add(1, Ordering::Relaxed);
}

#[doc(hidden)]
pub fn record_export_rejection(reason: Rejection) {
    reject(reason);
}

#[must_use]
pub fn facade_health() -> FacadeHealth {
    let load = |index: usize| REJECTIONS[index].load(Ordering::Relaxed);
    FacadeHealth {
        unknown_name: load(0),
        unknown_attribute: load(1),
        invalid_value: load(2),
        privacy: load(3),
        cardinality: load(4),
        size_limit: load(5),
    }
}
