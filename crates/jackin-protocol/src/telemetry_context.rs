// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Versioned cross-process telemetry correlation envelope.

use serde::{Deserialize, Serialize};

/// Current telemetry envelope wire version.
pub const TELEMETRY_CONTEXT_VERSION: u16 = 1;

/// W3C trace context plus bounded product correlation identifiers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryContext {
    /// Envelope version (`1` for this layout).
    pub v: u16,
    /// W3C traceparent. Malformed values are ignored by receivers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    /// W3C tracestate. Baggage is deliberately unsupported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
    /// CLI invocation UUID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,
    /// Session identifier (opaque, non-empty, at most 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Detached-job UUID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

impl TelemetryContext {
    /// Construct an empty v1 envelope.
    #[must_use]
    pub const fn v1() -> Self {
        Self {
            v: TELEMETRY_CONTEXT_VERSION,
            traceparent: None,
            tracestate: None,
            invocation_id: None,
            session_id: None,
            job_id: None,
        }
    }
}

impl jackin_telemetry::propagation::Carrier for TelemetryContext {
    fn version(&self) -> u16 {
        self.v
    }
    fn traceparent(&self) -> Option<&str> {
        self.traceparent.as_deref()
    }
    fn tracestate(&self) -> Option<&str> {
        self.tracestate.as_deref()
    }
    fn invocation_id(&self) -> Option<&str> {
        self.invocation_id.as_deref()
    }
    fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    fn job_id(&self) -> Option<&str> {
        self.job_id.as_deref()
    }
    fn set_trace(&mut self, traceparent: String, tracestate: Option<String>) {
        self.v = TELEMETRY_CONTEXT_VERSION;
        self.traceparent = Some(traceparent);
        self.tracestate = tracestate;
    }
    fn set_product_ids(
        &mut self,
        invocation_id: Option<String>,
        session_id: Option<String>,
        job_id: Option<String>,
    ) {
        self.invocation_id = invocation_id;
        self.session_id = session_id;
        self.job_id = job_id;
    }
}

#[cfg(test)]
mod tests;
