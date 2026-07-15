// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::{logs::AnyValue, trace::Status};

use crate::observability::TestExport;

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
                attribute_equals("event.name", event_name)
                    && attribute_equals("event.outcome", "failure")
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
}
