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
    let session = carrier.session_id().is_none_or(|value| {
        !value.is_empty()
            && value.len() <= 64
            && value
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || "-_.".contains(ch))
    });
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
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestCarrier {
        v: u16,
        parent: Option<String>,
        state: Option<String>,
        invocation: Option<String>,
        session: Option<String>,
        job: Option<String>,
    }
    impl Carrier for TestCarrier {
        fn version(&self) -> u16 {
            self.v
        }
        fn traceparent(&self) -> Option<&str> {
            self.parent.as_deref()
        }
        fn tracestate(&self) -> Option<&str> {
            self.state.as_deref()
        }
        fn invocation_id(&self) -> Option<&str> {
            self.invocation.as_deref()
        }
        fn session_id(&self) -> Option<&str> {
            self.session.as_deref()
        }
        fn job_id(&self) -> Option<&str> {
            self.job.as_deref()
        }
        fn set_trace(&mut self, parent: String, state: Option<String>) {
            self.parent = Some(parent);
            self.state = state;
        }
        fn set_product_ids(
            &mut self,
            invocation: Option<String>,
            session: Option<String>,
            job: Option<String>,
        ) {
            self.invocation = invocation;
            self.session = session;
            self.job = job;
        }
    }

    #[test]
    fn extraction_matrix_honors_unsampled_and_rejects_product_ids() {
        let valid = TestCarrier {
            v: 1,
            parent: Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00".into()),
            ..Default::default()
        };
        let ExtractOutcome::Parent(parent) = extract(&valid) else {
            panic!("valid parent")
        };
        assert!(!parent.is_sampled());
        let malformed = TestCarrier {
            v: 1,
            parent: Some("bad".into()),
            ..Default::default()
        };
        assert_eq!(extract(&malformed), ExtractOutcome::LocalRoot);
        let invalid_id = TestCarrier {
            v: 1,
            invocation: Some("not-a-uuid".into()),
            ..Default::default()
        };
        assert_eq!(extract(&invalid_id), ExtractOutcome::RejectRequest);
    }
}
