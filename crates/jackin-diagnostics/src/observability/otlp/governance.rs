// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use opentelemetry::logs::LogRecord as _;
use opentelemetry_sdk::Resource;

use super::EXPORT_TARGETS;

#[derive(Debug)]
pub(super) struct GovernedLogProcessor<P>(pub(super) P);

impl<P: opentelemetry_sdk::logs::LogProcessor> opentelemetry_sdk::logs::LogProcessor
    for GovernedLogProcessor<P>
{
    fn emit(
        &self,
        record: &mut opentelemetry_sdk::logs::SdkLogRecord,
        instrumentation: &opentelemetry::InstrumentationScope,
    ) {
        let governed = instrumentation.name() == jackin_telemetry::TELEMETRY_TARGET
            || record
                .target()
                .is_some_and(|target| EXPORT_TARGETS.contains(&target.as_ref()));
        if governed {
            let Some(event_name) = record.event_name() else {
                reject(jackin_telemetry::Rejection::UnknownName);
                return;
            };
            let Some(canonical_severity) = jackin_telemetry::event::canonical_severity(event_name)
            else {
                reject(jackin_telemetry::Rejection::UnknownName);
                return;
            };
            if record.severity_number() != Some(otel_log_severity(canonical_severity)) {
                reject(jackin_telemetry::Rejection::InvalidValue);
                return;
            }
            for (key, values) in jackin_telemetry::event::take_pending_event_arrays() {
                record.add_attribute(
                    key,
                    opentelemetry::logs::AnyValue::ListAny(Box::new(
                        values
                            .into_iter()
                            .map(|value| opentelemetry::logs::AnyValue::String(value.into()))
                            .collect(),
                    )),
                );
            }
        }
        if governed
            && record
                .attributes_iter()
                .any(|(key, _)| jackin_telemetry::privacy::validate_key(key.as_str()).is_err())
        {
            reject(jackin_telemetry::Rejection::UnknownAttribute);
            return;
        }
        self.0.emit(record, instrumentation);
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.force_flush()
    }

    fn shutdown_with_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.0.set_resource(resource);
    }
}

#[derive(Debug)]
pub(super) struct GovernedSpanProcessor<P>(pub(super) P);

impl<P: opentelemetry_sdk::trace::SpanProcessor> opentelemetry_sdk::trace::SpanProcessor
    for GovernedSpanProcessor<P>
{
    fn on_start(
        &self,
        span: &mut opentelemetry_sdk::trace::Span,
        context: &opentelemetry::Context,
    ) {
        self.0.on_start(span, context);
    }

    fn on_end(&self, span: opentelemetry_sdk::trace::SpanData) {
        if !jackin_telemetry::schema::spans::ALL.contains(&span.name.as_ref()) {
            reject(jackin_telemetry::Rejection::UnknownName);
            return;
        }
        if span.attributes.iter().any(|attribute| {
            jackin_telemetry::privacy::validate_key(attribute.key.as_str()).is_err()
        }) {
            reject(jackin_telemetry::Rejection::UnknownAttribute);
            return;
        }
        self.0.on_end(span);
    }

    fn force_flush(&self) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.force_flush()
    }

    fn shutdown_with_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.0.set_resource(resource);
    }
}

fn reject(rejection: jackin_telemetry::Rejection) {
    jackin_telemetry::record_export_rejection(rejection);
}

const fn otel_log_severity(
    severity: jackin_telemetry::event::Severity,
) -> opentelemetry::logs::Severity {
    match severity {
        jackin_telemetry::event::Severity::Trace => opentelemetry::logs::Severity::Trace,
        jackin_telemetry::event::Severity::Debug => opentelemetry::logs::Severity::Debug,
        jackin_telemetry::event::Severity::Info => opentelemetry::logs::Severity::Info,
        jackin_telemetry::event::Severity::Warn => opentelemetry::logs::Severity::Warn,
        jackin_telemetry::event::Severity::Error => opentelemetry::logs::Severity::Error,
    }
}
