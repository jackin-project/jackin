// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::{logs::AnyValue, trace::Status};

use crate::observability::TestExport;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestSpanSnapshot {
    pub name: String,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub sampled: bool,
    pub error: bool,
}

impl TestExport {
    pub fn force_flush(&self) {
        self.tracer_provider
            .force_flush()
            .into_iter()
            .for_each(drop);
        self.logger_provider
            .force_flush()
            .into_iter()
            .for_each(drop);
    }

    pub fn typed_error_count(&self, event_name: &str, error_type: &str) -> usize {
        self.logs
            .get_emitted_logs()
            .unwrap_or_default()
            .iter()
            .filter(|log| {
                let attribute_equals = |key: &str, expected: &str| {
                    log.record.attributes_iter().any(|(attribute, value)| {
                        attribute.as_str() == key
                            && matches!(value, AnyValue::String(actual) if actual.as_str() == expected)
                    })
                };
                log.record.event_name() == Some(event_name)
                    && attribute_equals("outcome", "failure")
                    && attribute_equals("error.type", error_type)
            })
            .count()
    }

    pub fn error_span_count(&self) -> usize {
        self.spans
            .get_finished_spans()
            .unwrap_or_default()
            .iter()
            .filter(|span| matches!(span.status, Status::Error { .. }))
            .count()
    }

    pub fn contains_log_text(&self, needle: &str) -> bool {
        self.logs
            .get_emitted_logs()
            .unwrap_or_default()
            .iter()
            .any(|log| {
                log.record
                    .body()
                    .is_some_and(|body| format!("{body:?}").contains(needle))
                    || log.record.attributes_iter().any(|(key, value)| {
                        key.as_str().contains(needle) || format!("{value:?}").contains(needle)
                    })
            })
    }

    pub fn contains_span_text(&self, needle: &str) -> bool {
        self.spans
            .get_finished_spans()
            .unwrap_or_default()
            .iter()
            .any(|span| {
                span.name.contains(needle)
                    || span.attributes.iter().any(|attribute| {
                        attribute.key.as_str().contains(needle)
                            || format!("{:?}", attribute.value).contains(needle)
                    })
            })
    }

    pub fn finished_spans(&self) -> Vec<TestSpanSnapshot> {
        self.spans
            .get_finished_spans()
            .unwrap_or_default()
            .into_iter()
            .map(|span| TestSpanSnapshot {
                name: span.name.into_owned(),
                trace_id: span.span_context.trace_id().to_string(),
                span_id: span.span_context.span_id().to_string(),
                parent_span_id: span.parent_span_id.to_string(),
                sampled: span.span_context.is_sampled(),
                error: matches!(span.status, Status::Error { .. }),
            })
            .collect()
    }
}
