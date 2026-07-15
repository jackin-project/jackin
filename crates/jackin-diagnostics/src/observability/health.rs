// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Lock-free outer telemetry health counters.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

static ACTIVE_SIGNALS: AtomicU8 = AtomicU8::new(0);
static EXPORT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static EXPORT_SUCCESSES: AtomicU64 = AtomicU64::new(0);
static EXPORT_FAILURES: AtomicU64 = AtomicU64::new(0);
static TRACE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static TRACE_SUCCESSES: AtomicU64 = AtomicU64::new(0);
static TRACE_FAILURES: AtomicU64 = AtomicU64::new(0);
static LOG_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static LOG_SUCCESSES: AtomicU64 = AtomicU64::new(0);
static LOG_FAILURES: AtomicU64 = AtomicU64::new(0);
static METRIC_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static METRIC_SUCCESSES: AtomicU64 = AtomicU64::new(0);
static METRIC_FAILURES: AtomicU64 = AtomicU64::new(0);
static FACADE_REJECTIONS: AtomicU64 = AtomicU64::new(0);
static SHUTDOWN_COMPLETED: AtomicBool = AtomicBool::new(false);
static SHUTDOWN_SUCCEEDED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetrySignalHealth {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetryHealth {
    pub active_signals: u8,
    pub export_attempts: u64,
    pub export_successes: u64,
    pub export_failures: u64,
    pub traces: TelemetrySignalHealth,
    pub logs: TelemetrySignalHealth,
    pub metrics: TelemetrySignalHealth,
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
        traces: signal_snapshot(&TRACE_ATTEMPTS, &TRACE_SUCCESSES, &TRACE_FAILURES),
        logs: signal_snapshot(&LOG_ATTEMPTS, &LOG_SUCCESSES, &LOG_FAILURES),
        metrics: signal_snapshot(&METRIC_ATTEMPTS, &METRIC_SUCCESSES, &METRIC_FAILURES),
        facade_rejections: FACADE_REJECTIONS.load(Ordering::Relaxed),
        shutdown_completed: SHUTDOWN_COMPLETED.load(Ordering::Relaxed),
        shutdown_succeeded: SHUTDOWN_SUCCEEDED.load(Ordering::Relaxed),
    }
}

fn signal_snapshot(
    attempts: &AtomicU64,
    successes: &AtomicU64,
    failures: &AtomicU64,
) -> TelemetrySignalHealth {
    TelemetrySignalHealth {
        attempts: attempts.load(Ordering::Relaxed),
        successes: successes.load(Ordering::Relaxed),
        failures: failures.load(Ordering::Relaxed),
    }
}

pub(super) enum Signal {
    Traces,
    Logs,
    Metrics,
}

pub(super) fn record_signal_export(signal: Signal, succeeded: bool) {
    let (attempts, successes, failures) = match signal {
        Signal::Traces => (&TRACE_ATTEMPTS, &TRACE_SUCCESSES, &TRACE_FAILURES),
        Signal::Logs => (&LOG_ATTEMPTS, &LOG_SUCCESSES, &LOG_FAILURES),
        Signal::Metrics => (&METRIC_ATTEMPTS, &METRIC_SUCCESSES, &METRIC_FAILURES),
    };
    attempts.fetch_add(1, Ordering::Relaxed);
    if succeeded {
        successes.fetch_add(1, Ordering::Relaxed);
    } else {
        failures.fetch_add(1, Ordering::Relaxed);
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
