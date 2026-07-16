// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Paired launch-stage telemetry shared by rich and headless launch paths.

use std::time::Instant;

use crate::schema::enums::{ErrorType, LaunchStageName, LaunchTargetKind, OutcomeValue};
use crate::{Attr, FieldSet, OperationGuard, Value};

#[derive(Debug)]
pub struct StageGuard {
    stage: LaunchStageName,
    target: LaunchTargetKind,
    operation: Option<OperationGuard>,
    started_at: Instant,
}

impl StageGuard {
    #[must_use]
    pub fn start(stage: LaunchStageName, target: LaunchTargetKind) -> Self {
        let stage_name = stage.as_str();
        let stage_attr = [Attr {
            key: crate::schema::attrs::LAUNCH_STAGE_NAME,
            value: Value::Str(stage_name),
        }];
        let operation = crate::operation_or_disabled(&crate::operation::LAUNCH_STAGE, &stage_attr);
        let target_name = target.as_str();
        let active_attrs = [
            stage_attr[0],
            Attr {
                key: crate::schema::attrs::LAUNCH_TARGET_KIND,
                value: Value::Str(target_name),
            },
        ];
        let _active =
            crate::up_down_counter(&crate::metric::LAUNCH_STAGE_ACTIVE).add(1, &active_attrs);
        let event_attrs = [
            stage_attr[0],
            Attr {
                key: crate::schema::attrs::OUTCOME,
                value: Value::Str(OutcomeValue::Success.as_str()),
            },
        ];
        let _started = crate::emit_event(
            &crate::event::LAUNCH_STAGE_STARTED,
            FieldSet::new(&event_attrs, None),
        );
        Self {
            stage,
            target,
            operation: Some(operation),
            started_at: Instant::now(),
        }
    }

    pub fn complete(mut self, outcome: OutcomeValue, error_type: Option<ErrorType>) {
        self.finish(outcome, error_type);
    }

    fn finish(&mut self, outcome: OutcomeValue, error_type: Option<ErrorType>) {
        let Some(operation) = self.operation.take() else {
            return;
        };
        operation.complete(outcome, error_type);

        let stage_name = self.stage.as_str();
        let target_name = self.target.as_str();
        let active_attrs = [
            Attr {
                key: crate::schema::attrs::LAUNCH_STAGE_NAME,
                value: Value::Str(stage_name),
            },
            Attr {
                key: crate::schema::attrs::LAUNCH_TARGET_KIND,
                value: Value::Str(target_name),
            },
        ];
        let _active =
            crate::up_down_counter(&crate::metric::LAUNCH_STAGE_ACTIVE).add(-1, &active_attrs);

        let mut terminal_attrs = vec![
            active_attrs[0],
            active_attrs[1],
            Attr {
                key: crate::schema::attrs::OUTCOME,
                value: Value::Str(outcome.as_str()),
            },
        ];
        if let Some(error_type) = error_type {
            terminal_attrs.push(Attr {
                key: crate::schema::attrs::std_attrs::ERROR_TYPE,
                value: Value::Str(error_type.as_str()),
            });
        }
        let _executions =
            crate::counter(&crate::metric::LAUNCH_STAGE_EXECUTIONS).add(1, &terminal_attrs);
        let _duration = crate::histogram(&crate::metric::LAUNCH_STAGE_DURATION)
            .record(self.started_at.elapsed().as_secs_f64(), &terminal_attrs);

        let event = match outcome {
            OutcomeValue::Success => &crate::event::LAUNCH_STAGE_DONE,
            OutcomeValue::Skip | OutcomeValue::Cancellation => &crate::event::LAUNCH_STAGE_SKIPPED,
            OutcomeValue::Failure | OutcomeValue::Error | OutcomeValue::Timeout => {
                &crate::event::LAUNCH_STAGE_FAILED
            }
        };
        let _terminal = crate::emit_event(event, FieldSet::new(&terminal_attrs, None));
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        self.finish(
            OutcomeValue::Error,
            Some(ErrorType::TelemetryInstrumentationFault),
        );
    }
}
