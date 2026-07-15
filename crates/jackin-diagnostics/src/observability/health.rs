// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Lock-free outer telemetry health counters.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

static ACTIVE_SIGNALS: AtomicU8 = AtomicU8::new(0);
static EXPORT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static EXPORT_SUCCESSES: AtomicU64 = AtomicU64::new(0);
static EXPORT_FAILURES: AtomicU64 = AtomicU64::new(0);
static FACADE_REJECTIONS: AtomicU64 = AtomicU64::new(0);
static SHUTDOWN_COMPLETED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_SUCCEEDED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetryHealth {
    pub active_signals: u8,
    pub export_attempts: u64,
    pub export_successes: u64,
    pub export_failures: u64,
    pub facade_rejections: u64,
    pub shutdown_completed: bool,
    pub shutdown_succeeded: bool,
}

#[must_use]
pub fn telemetry_health_snapshot() -> TelemetryHealth {
    TelemetryHealth {
        active_signals: ACTIVE_SIGNALS.load(Ordering::Relaxed),
        export_attempts: EXPORT_ATTEMPTS.load(Ordering::Relaxed),
        export_successes: EXPORT_SUCCESSES.load(Ordering::Relaxed),
        export_failures: EXPORT_FAILURES.load(Ordering::Relaxed),
        facade_rejections: FACADE_REJECTIONS.load(Ordering::Relaxed),
        shutdown_completed: SHUTDOWN_COMPLETED.load(Ordering::Relaxed),
        shutdown_succeeded: SHUTDOWN_SUCCEEDED.load(Ordering::Relaxed),
    }
}

pub(super) fn set_active_signals(metrics: bool) {
    ACTIVE_SIGNALS.store(if metrics { 3 } else { 2 }, Ordering::Relaxed);
}

pub(super) fn record_export_attempt() {
    EXPORT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

pub(super) fn record_export_success() {
    EXPORT_SUCCESSES.fetch_add(1, Ordering::Relaxed);
}

pub(super) fn record_export_failure() {
    EXPORT_FAILURES.fetch_add(1, Ordering::Relaxed);
}

pub fn record_telemetry_rejection() {
    FACADE_REJECTIONS.fetch_add(1, Ordering::Relaxed);
}

pub(super) fn record_shutdown(succeeded: bool) {
    SHUTDOWN_SUCCEEDED.store(succeeded, Ordering::Relaxed);
    SHUTDOWN_COMPLETED.store(true, Ordering::Release);
    ACTIVE_SIGNALS.store(0, Ordering::Relaxed);
}

#[cfg(test)]
mod tests;
