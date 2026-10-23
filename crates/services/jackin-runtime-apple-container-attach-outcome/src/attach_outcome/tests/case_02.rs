// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt::{self, Display};
use std::sync::{Arc, Mutex};

use super::super::diagnostics::{self, AttachOutcomeDiagnostic, UncertainObservedState};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

#[derive(Clone, Debug, Eq, PartialEq)]
struct CapturedEvent {
    name: String,
    level: String,
    fields: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct EventCapture(Arc<Mutex<Vec<CapturedEvent>>>);

impl<S> Layer<S> for EventCapture
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let mut visitor = EventFields::default();
        event.record(&mut visitor);
        self.0.lock().unwrap().push(CapturedEvent {
            name: event.metadata().name().to_owned(),
            level: event.metadata().level().to_string(),
            fields: visitor.0,
        });
    }
}

#[derive(Default)]
struct EventFields(BTreeMap<String, String>);

impl Visit for EventFields {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0.insert(field.name().to_owned(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_owned(), value.to_owned());
    }
}

#[derive(Default)]
struct DisplayCanary(std::cell::Cell<usize>);

impl Display for DisplayCanary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.set(self.0.get() + 1);
        formatter.write_str("apple-inventory-display-canary")
    }
}

#[test]
fn disabled_warning_does_not_format_the_original_error() {
    let error = DisplayCanary::default();
    let subscriber =
        tracing_subscriber::registry().with(tracing_subscriber::filter::LevelFilter::OFF);
    tracing::subscriber::with_default(subscriber, || {
        diagnostics::emit(
            "jackin-a",
            AttachOutcomeDiagnostic::InspectionUncertain(&error),
        );
    });
    assert_eq!(error.0.get(), 0);
}

#[test]
fn enabled_warning_events_preserve_bounded_reason_state_and_error_context() {
    use jackin_telemetry::schema::enums::AppleContainerAttachOutcomeReason as Reason;

    let inspection_error = String::from("inspection-error-canary");
    let recording_error = String::from("recording-error-canary");
    let cases = [
        (
            AttachOutcomeDiagnostic::InspectionUncertain(&inspection_error),
            Reason::InspectionUncertain.as_str(),
            None,
            Some("inspection-error-canary"),
            "skipping Apple Container attach outcome because inspection was uncertain",
        ),
        (
            AttachOutcomeDiagnostic::Absent,
            Reason::Absent.as_str(),
            None,
            None,
            "skipping Apple Container attach outcome because the exact ID is absent",
        ),
        (
            AttachOutcomeDiagnostic::StateUncertain(UncertainObservedState::Unknown),
            Reason::StateUncertain.as_str(),
            Some("unknown"),
            None,
            "skipping Apple Container attach outcome because its state is uncertain",
        ),
        (
            AttachOutcomeDiagnostic::StateUncertain(UncertainObservedState::Stopping),
            Reason::StateUncertain.as_str(),
            Some("stopping"),
            None,
            "skipping Apple Container attach outcome because its state is uncertain",
        ),
        (
            AttachOutcomeDiagnostic::RecordingFailed(&recording_error),
            Reason::RecordingFailed.as_str(),
            None,
            Some("recording-error-canary"),
            "Apple Container attach outcome recording failed",
        ),
    ];
    let capture = EventCapture::default();
    let subscriber = tracing_subscriber::registry().with(capture.clone());
    tracing::subscriber::with_default(subscriber, || {
        for (diagnostic, _, _, _, _) in cases {
            diagnostics::emit("jackin-observed", diagnostic);
        }
    });

    let records = capture.0.lock().unwrap();
    assert_eq!(records.len(), cases.len());
    for (record, (_, reason, state, exception, message)) in records.iter().zip(cases) {
        assert_eq!(
            record.name,
            "runtime.apple_container.attach_outcome.warning"
        );
        assert_eq!(record.level, Level::WARN.to_string());
        assert_eq!(
            record.fields.get("message").map(String::as_str),
            Some(message)
        );
        assert_eq!(
            record.fields.get("container.id").map(String::as_str),
            Some("jackin-observed")
        );
        assert_eq!(
            record
                .fields
                .get("apple.container.attach_outcome.reason")
                .map(String::as_str),
            Some(reason)
        );
        assert_eq!(
            record
                .fields
                .get("apple.container.state")
                .map(String::as_str),
            state
        );
        assert_eq!(
            record.fields.get("exception.message").map(String::as_str),
            exception
        );
    }
}

#[test]
fn event_contract_rejects_unregistered_reason_values() {
    use jackin_telemetry::event::RUNTIME_APPLE_CONTAINER_ATTACH_OUTCOME_WARNING;
    use jackin_telemetry::schema::attrs::{APPLE_CONTAINER_ATTACH_OUTCOME_REASON, std_attrs};
    use jackin_telemetry::{Attr, FieldSet, Rejection, Value, emit_event};

    let fields = [
        Attr {
            key: std_attrs::CONTAINER_ID,
            value: Value::Str("jackin-a"),
        },
        Attr {
            key: APPLE_CONTAINER_ATTACH_OUTCOME_REASON,
            value: Value::Str("future-unregistered-reason"),
        },
    ];
    let subscriber = tracing_subscriber::registry().with(EventCapture::default());
    let result = tracing::subscriber::with_default(subscriber, || {
        emit_event(
            &RUNTIME_APPLE_CONTAINER_ATTACH_OUTCOME_WARNING,
            FieldSet::new(&fields, None),
        )
    });
    assert_eq!(result, Err(Rejection::InvalidValue));
}
