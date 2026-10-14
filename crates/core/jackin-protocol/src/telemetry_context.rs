// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};

pub const TELEMETRY_CONTEXT_VERSION: u16 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TelemetryContext {
    pub v: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracestate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invocation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
}

impl TelemetryContext {
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
