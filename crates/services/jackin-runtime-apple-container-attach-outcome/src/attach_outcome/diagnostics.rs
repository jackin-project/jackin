// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use jackin_telemetry::{
    Attr, FieldSet, Value, emit_event,
    event::RUNTIME_APPLE_CONTAINER_ATTACH_OUTCOME_WARNING,
    event_enabled,
    schema::{
        attrs::{APPLE_CONTAINER_ATTACH_OUTCOME_REASON, APPLE_CONTAINER_STATE, std_attrs},
        enums::{AppleContainerAttachOutcomeReason, AppleContainerState},
    },
};

#[derive(Clone, Copy)]
pub(super) enum UncertainObservedState {
    Unknown,
    Stopping,
}

#[derive(Clone, Copy)]
pub(super) enum AttachOutcomeDiagnostic<'a> {
    InspectionUncertain(&'a dyn Display),
    Absent,
    StateUncertain(UncertainObservedState),
    RecordingFailed(&'a dyn Display),
}

impl<'a> AttachOutcomeDiagnostic<'a> {
    const fn reason(self) -> AppleContainerAttachOutcomeReason {
        match self {
            Self::InspectionUncertain(_) => AppleContainerAttachOutcomeReason::InspectionUncertain,
            Self::Absent => AppleContainerAttachOutcomeReason::Absent,
            Self::StateUncertain(_) => AppleContainerAttachOutcomeReason::StateUncertain,
            Self::RecordingFailed(_) => AppleContainerAttachOutcomeReason::RecordingFailed,
        }
    }

    const fn observed_state(self) -> Option<AppleContainerState> {
        match self {
            Self::StateUncertain(UncertainObservedState::Unknown) => {
                Some(AppleContainerState::Unknown)
            }
            Self::StateUncertain(UncertainObservedState::Stopping) => {
                Some(AppleContainerState::Stopping)
            }
            Self::InspectionUncertain(_) | Self::Absent | Self::RecordingFailed(_) => None,
        }
    }

    fn error(self) -> Option<&'a (dyn Display + 'a)> {
        match self {
            Self::InspectionUncertain(error) | Self::RecordingFailed(error) => Some(error),
            Self::Absent | Self::StateUncertain(_) => None,
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::InspectionUncertain(_) => {
                "skipping Apple Container attach outcome because inspection was uncertain"
            }
            Self::Absent => {
                "skipping Apple Container attach outcome because the exact ID is absent"
            }
            Self::StateUncertain(_) => {
                "skipping Apple Container attach outcome because its state is uncertain"
            }
            Self::RecordingFailed(_) => "Apple Container attach outcome recording failed",
        }
    }

    fn fields<'fields>(
        self,
        container_id: &'fields str,
        exception_message: Option<&'fields str>,
    ) -> Vec<Attr<'fields>> {
        let mut attrs = vec![
            Attr {
                key: std_attrs::CONTAINER_ID,
                value: Value::Str(container_id),
            },
            Attr {
                key: APPLE_CONTAINER_ATTACH_OUTCOME_REASON,
                value: Value::Str(self.reason().as_str()),
            },
        ];
        if let Some(state) = self.observed_state() {
            attrs.push(Attr {
                key: APPLE_CONTAINER_STATE,
                value: Value::Str(state.as_str()),
            });
        }
        if matches!(
            self,
            Self::InspectionUncertain(_) | Self::RecordingFailed(_)
        ) && let Some(message) = exception_message
        {
            attrs.push(Attr {
                key: std_attrs::EXCEPTION_MESSAGE,
                value: Value::Str(message),
            });
        }
        attrs
    }
}

pub(super) fn emit(container_id: &str, diagnostic: AttachOutcomeDiagnostic<'_>) {
    if !event_enabled(&RUNTIME_APPLE_CONTAINER_ATTACH_OUTCOME_WARNING) {
        return;
    }
    let exception_message = diagnostic.error().map(ToString::to_string);
    let attrs = diagnostic.fields(container_id, exception_message.as_deref());
    let _event_result = emit_event(
        &RUNTIME_APPLE_CONTAINER_ATTACH_OUTCOME_WARNING,
        FieldSet::new(&attrs, Some(diagnostic.message())),
    );
}
