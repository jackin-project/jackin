// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for observability setup.

use super::{
    TelemetryFlushStatus, TelemetryHealth, TelemetrySignalHealth, ValidationFailure,
    rewrite_endpoint_for_container, validate_delivery_delta,
};

#[test]
fn loopback_is_rewritten_to_host_gateway() {
    let rewritten = rewrite_endpoint_for_container("http://127.0.0.1:4318");
    assert_eq!(rewritten.endpoint, "http://host.docker.internal:4318");
    assert!(rewritten.needs_host_gateway);

    let with_path = rewrite_endpoint_for_container("http://localhost:4318/v1/traces");
    assert_eq!(
        with_path.endpoint,
        "http://host.docker.internal:4318/v1/traces"
    );
    assert!(with_path.needs_host_gateway);
}

#[test]
fn routable_host_is_left_alone() {
    let rewritten = rewrite_endpoint_for_container("http://otel.internal:4318");
    assert_eq!(rewritten.endpoint, "http://otel.internal:4318");
    assert!(!rewritten.needs_host_gateway);
}

fn validation_health(successes: u64) -> TelemetryHealth {
    let signal = TelemetrySignalHealth {
        attempts: successes,
        successes,
        failures: 0,
    };
    TelemetryHealth {
        active_signals: 3,
        export_attempts: successes * 3,
        export_successes: successes * 3,
        export_failures: 0,
        traces: signal,
        logs: signal,
        metrics: signal,
        facade_rejections: 0,
        flush: TelemetryFlushStatus::Succeeded,
        shutdown_completed: false,
        shutdown_succeeded: false,
        shutdown_timed_out: false,
    }
}

#[test]
fn validation_requires_a_new_success_for_every_signal() {
    let before = validation_health(7);
    let mut after = validation_health(8);
    assert_eq!(validate_delivery_delta(before, after), Ok(()));

    after.logs.successes = before.logs.successes;
    assert_eq!(
        validate_delivery_delta(before, after),
        Err(ValidationFailure::Export("logs"))
    );
}

#[test]
fn validation_rejects_new_failures_rejections_and_failed_flush() {
    let before = validation_health(7);

    let mut failure = validation_health(8);
    failure.metrics.failures = 1;
    assert_eq!(
        validate_delivery_delta(before, failure),
        Err(ValidationFailure::Export("metrics"))
    );

    let mut rejected = validation_health(8);
    rejected.facade_rejections = 1;
    assert_eq!(
        validate_delivery_delta(before, rejected),
        Err(ValidationFailure::Rejected)
    );

    let mut flush = validation_health(8);
    flush.flush = TelemetryFlushStatus::Failed;
    assert_eq!(
        validate_delivery_delta(before, flush),
        Err(ValidationFailure::Export("flush"))
    );
}
