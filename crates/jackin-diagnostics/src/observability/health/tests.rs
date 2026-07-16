// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{TelemetryFlushStatus, record_telemetry_rejection, telemetry_health_snapshot};

#[test]
fn facade_rejection_is_visible_in_snapshot() {
    let before = telemetry_health_snapshot().facade_rejections;
    jackin_telemetry::record_export_rejection(jackin_telemetry::Rejection::Privacy);
    assert_eq!(telemetry_health_snapshot().facade_rejections, before + 1);

    let before = telemetry_health_snapshot().facade_rejections;
    record_telemetry_rejection();
    assert_eq!(telemetry_health_snapshot().facade_rejections, before + 1);
}

#[test]
fn provider_lifecycle_reports_three_signals_flush_and_shutdown() {
    let _test_lock = super::TEST_STATE_LOCK.lock().expect("health test lock");
    let generation = super::set_active_signals();
    assert_eq!(telemetry_health_snapshot().active_signals, 3);
    assert_eq!(
        telemetry_health_snapshot().flush,
        TelemetryFlushStatus::Pending
    );

    super::record_flush(generation, false);
    let flushed = telemetry_health_snapshot();
    assert_eq!(flushed.flush, TelemetryFlushStatus::Failed);

    super::record_shutdown(generation, false);
    let shutdown = telemetry_health_snapshot();
    assert_eq!(shutdown.active_signals, 0);
    assert!(shutdown.shutdown_completed);
    assert!(!shutdown.shutdown_succeeded);
}

#[test]
fn stale_generation_cannot_overwrite_current_lifecycle() {
    let _test_lock = super::TEST_STATE_LOCK.lock().expect("health test lock");
    let stale = super::set_active_signals();
    let current = super::set_active_signals();
    super::record_shutdown(stale, true);
    assert_eq!(telemetry_health_snapshot().active_signals, 3);
    super::record_shutdown_timeout(current);
    let timed_out = telemetry_health_snapshot();
    assert!(timed_out.shutdown_timed_out);
    assert!(!timed_out.shutdown_completed);
}
