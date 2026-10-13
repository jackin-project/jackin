// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Isolation boundary for the experimental OTLP/gRPC retry API.

use std::time::Duration;

pub(super) fn policy() -> opentelemetry_otlp::RetryPolicy {
    opentelemetry_otlp::RetryPolicy::default()
        .with_max_retries(2)
        .with_initial_delay(Duration::from_millis(250))
        .with_max_delay(Duration::from_millis(1_000))
        .with_max_jitter(Duration::from_millis(100))
}
