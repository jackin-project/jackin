// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Isolation boundary for the experimental OTLP/gRPC retry API.

pub(super) const fn policy() -> opentelemetry_otlp::RetryPolicy {
    opentelemetry_otlp::RetryPolicy {
        max_retries: 2,
        initial_delay_ms: 250,
        max_delay_ms: 1_000,
        jitter_ms: 100,
    }
}
