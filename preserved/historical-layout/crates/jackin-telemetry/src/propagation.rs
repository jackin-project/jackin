// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr as _;

use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};
use tracing_opentelemetry::OpenTelemetrySpanExt as _;

pub const VERSION: u16 = 1;

pub trait Carrier {
    fn version(&self) -> u16;
    fn traceparent(&self) -> Option<&str>;
    fn tracestate(&self) -> Option<&str>;
    fn invocation_id(&self) -> Option<&str>;
    fn session_id(&self) -> Option<&str>;
    fn job_id(&self) -> Option<&str>;
    fn set_trace(&mut self, traceparent: String, tracestate: Option<String>);
    fn set_product_ids(
        &mut self,
        invocation_id: Option<String>,
        session_id: Option<String>,
        job_id: Option<String>,
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtractOutcome {
    Parent(SpanContext),
    LocalRoot,
    RejectRequest,
}

pub fn inject(carrier: &mut impl Carrier) {
    carrier.set_product_ids(
        crate::identity::current_invocation().map(|id| id.to_string()),
        crate::identity::current_session().map(|session| session.current.to_string()),
        None,
    );
    use opentelemetry::trace::TraceContextExt as _;
    let context = tracing::Span::current().context();
    let span = context.span().span_context().clone();
    if !span.is_valid() {
        return;
    }
    let sampled = if span.is_sampled() { "01" } else { "00" };
    carrier.set_trace(
        format!("00-{}-{}-{sampled}", span.trace_id(), span.span_id()),
        (!span.trace_state().header().is_empty()).then(|| span.trace_state().header()),
    );
}

/// W3C traceparent for the currently entered bounded operation.
#[must_use]
pub fn current_traceparent() -> Option<String> {
    use opentelemetry::trace::TraceContextExt as _;
    let context = tracing::Span::current().context();
    let span = context.span().span_context().clone();
    span.is_valid().then(|| {
        let sampled = if span.is_sampled() { "01" } else { "00" };
        format!("00-{}-{}-{sampled}", span.trace_id(), span.span_id())
    })
}

pub fn extract(carrier: &impl Carrier) -> ExtractOutcome {
    if carrier.version() != VERSION || !valid_product_ids(carrier) {
        return ExtractOutcome::RejectRequest;
    }
    let Some(value) = carrier.traceparent() else {
        return ExtractOutcome::LocalRoot;
    };
    parse_traceparent(value, carrier.tracestate())
        .map_or(ExtractOutcome::LocalRoot, ExtractOutcome::Parent)
}

fn valid_product_ids(carrier: &impl Carrier) -> bool {
    let uuid = |value: Option<&str>| value.is_none_or(|value| uuid::Uuid::parse_str(value).is_ok());
    let session = carrier
        .session_id()
        .is_none_or(|value| !value.is_empty() && value.len() <= 64);
    uuid(carrier.invocation_id()) && uuid(carrier.job_id()) && session
}

fn parse_traceparent(value: &str, state: Option<&str>) -> Option<SpanContext> {
    let mut parts = value.split('-');
    if parts.next()? != "00" {
        return None;
    }
    let trace_id = parts.next()?;
    let span_id = parts.next()?;
    let flags = parts.next()?;
    if parts.next().is_some() || trace_id.len() != 32 || span_id.len() != 16 || flags.len() != 2 {
        return None;
    }
    let trace_state = state
        .map(TraceState::from_str)
        .transpose()
        .ok()?
        .unwrap_or_default();
    let context = SpanContext::new(
        TraceId::from_hex(trace_id).ok()?,
        SpanId::from_hex(span_id).ok()?,
        TraceFlags::new(u8::from_str_radix(flags, 16).ok()?),
        true,
        trace_state,
    );
    context.is_valid().then_some(context)
}

#[cfg(test)]
mod tests;
