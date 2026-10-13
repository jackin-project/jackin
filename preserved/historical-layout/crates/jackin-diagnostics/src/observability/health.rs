// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Lock-free outer telemetry health counters.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

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
static LIFECYCLE: Mutex<Lifecycle> = Mutex::new(Lifecycle::new());
#[cfg(test)]
pub(super) static TEST_STATE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy)]
struct Lifecycle {
    generation: u64,
    active_signals: u8,
    flush: TelemetryFlushStatus,
    shutdown_completed: bool,
    shutdown_succeeded: bool,
    shutdown_timed_out: bool,
}

impl Lifecycle {
    const fn new() -> Self {
        Self {
            generation: 0,
            active_signals: 0,
            flush: TelemetryFlushStatus::Pending,
            shutdown_completed: false,
            shutdown_succeeded: false,
            shutdown_timed_out: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetrySignalHealth {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryFlushStatus {
    Pending,
    Succeeded,
    Failed,
}

/// Why a Capsule process can or cannot export directly to OTLP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsuleExportCoverage {
    /// This process is not a Capsule.
    NotApplicable,
    /// Endpoint and authentication requirements are Capsule-safe.
    Enabled,
    /// No OTLP endpoint is configured on the host.
    DisabledNoEndpoint,
    /// The effective network policy forbids all egress.
    DisabledNetworkNone,
    /// The endpoint was not explicitly classified Capsule-safe.
    DisabledUnclassifiedEndpoint,
    /// Host authentication exists without a dedicated Capsule-safe carrier.
    DisabledUnclassifiedAuth,
}

impl CapsuleExportCoverage {
    pub const ENV_NAME: &'static str = "JACKIN_CAPSULE_OTLP_COVERAGE";

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::Enabled => "enabled",
            Self::DisabledNoEndpoint => "disabled_no_endpoint",
            Self::DisabledNetworkNone => "disabled_network_none",
            Self::DisabledUnclassifiedEndpoint => "disabled_unclassified_endpoint",
            Self::DisabledUnclassifiedAuth => "disabled_unclassified_auth",
        }
    }

    fn from_env() -> Self {
        match std::env::var(Self::ENV_NAME).as_deref() {
            Ok("enabled") => Self::Enabled,
            Ok("disabled_no_endpoint") => Self::DisabledNoEndpoint,
            Ok("disabled_network_none") => Self::DisabledNetworkNone,
            Ok("disabled_unclassified_endpoint") => Self::DisabledUnclassifiedEndpoint,
            Ok("disabled_unclassified_auth") => Self::DisabledUnclassifiedAuth,
            _ => Self::NotApplicable,
        }
    }
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
    pub capsule_export: CapsuleExportCoverage,
    pub flush: TelemetryFlushStatus,
    pub shutdown_completed: bool,
    pub shutdown_succeeded: bool,
    pub shutdown_timed_out: bool,
}

#[must_use]
pub fn telemetry_health_snapshot() -> TelemetryHealth {
    let lifecycle = *LIFECYCLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    TelemetryHealth {
        active_signals: lifecycle.active_signals,
        export_attempts: EXPORT_ATTEMPTS.load(Ordering::Relaxed),
        export_successes: EXPORT_SUCCESSES.load(Ordering::Relaxed),
        export_failures: EXPORT_FAILURES.load(Ordering::Relaxed),
        traces: signal_snapshot(&TRACE_ATTEMPTS, &TRACE_SUCCESSES, &TRACE_FAILURES),
        logs: signal_snapshot(&LOG_ATTEMPTS, &LOG_SUCCESSES, &LOG_FAILURES),
        metrics: signal_snapshot(&METRIC_ATTEMPTS, &METRIC_SUCCESSES, &METRIC_FAILURES),
        facade_rejections: FACADE_REJECTIONS.load(Ordering::Relaxed)
            + facade_rejection_count(jackin_telemetry::facade_health()),
        capsule_export: CapsuleExportCoverage::from_env(),
        flush: lifecycle.flush,
        shutdown_completed: lifecycle.shutdown_completed,
        shutdown_succeeded: lifecycle.shutdown_succeeded,
        shutdown_timed_out: lifecycle.shutdown_timed_out,
    }
}

const fn facade_rejection_count(health: jackin_telemetry::FacadeHealth) -> u64 {
    health.unknown_name
        + health.unknown_attribute
        + health.invalid_value
        + health.privacy
        + health.cardinality
        + health.size_limit
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
    EXPORT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    let (attempts, successes, failures) = match signal {
        Signal::Traces => (&TRACE_ATTEMPTS, &TRACE_SUCCESSES, &TRACE_FAILURES),
        Signal::Logs => (&LOG_ATTEMPTS, &LOG_SUCCESSES, &LOG_FAILURES),
        Signal::Metrics => (&METRIC_ATTEMPTS, &METRIC_SUCCESSES, &METRIC_FAILURES),
    };
    attempts.fetch_add(1, Ordering::Relaxed);
    if succeeded {
        EXPORT_SUCCESSES.fetch_add(1, Ordering::Relaxed);
        successes.fetch_add(1, Ordering::Relaxed);
    } else {
        EXPORT_FAILURES.fetch_add(1, Ordering::Relaxed);
        failures.fetch_add(1, Ordering::Relaxed);
    }
}

pub(super) fn set_active_signals() -> u64 {
    let mut state = LIFECYCLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.generation = state.generation.wrapping_add(1);
    state.active_signals = 3;
    state.flush = TelemetryFlushStatus::Pending;
    state.shutdown_completed = false;
    state.shutdown_succeeded = false;
    state.shutdown_timed_out = false;
    state.generation
}

pub(super) fn record_flush(generation: u64, succeeded: bool) {
    let mut state = LIFECYCLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.generation == generation {
        state.flush = if succeeded {
            TelemetryFlushStatus::Succeeded
        } else {
            TelemetryFlushStatus::Failed
        };
    }
}

pub fn record_telemetry_rejection() {
    FACADE_REJECTIONS.fetch_add(1, Ordering::Relaxed);
}

pub(super) fn record_shutdown_timeout(generation: u64) {
    let mut state = LIFECYCLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.generation == generation {
        state.shutdown_timed_out = true;
    }
}

pub(super) fn record_shutdown(generation: u64, succeeded: bool) {
    let mut state = LIFECYCLE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if state.generation == generation {
        state.shutdown_succeeded = succeeded;
        state.shutdown_completed = true;
        state.active_signals = 0;
    }
}

#[cfg(test)]
mod tests;
