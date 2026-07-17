// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Bounded stream and watcher lifecycle phases.

use crate::{Attr, OperationGuard, Value, autonomous_root_operation, operation, schema};

#[must_use]
pub fn phase(phase: schema::enums::StreamOperation) -> Option<OperationGuard> {
    let attrs = [Attr {
        key: schema::attrs::STREAM_OPERATION,
        value: Value::Str(phase.as_str()),
    }];
    autonomous_root_operation(&operation::STREAM_OPERATION, &attrs).ok()
}

pub fn complete_success(operation: Option<OperationGuard>) {
    if let Some(operation) = operation {
        operation.complete(schema::enums::OutcomeValue::Success, None);
    }
}

pub fn complete_error(operation: Option<OperationGuard>, error_type: schema::enums::ErrorType) {
    if let Some(operation) = operation {
        operation.complete(schema::enums::OutcomeValue::Error, Some(error_type));
    }
}

#[derive(Debug)]
pub struct CloseOnDrop {
    completed: bool,
}

#[must_use]
pub const fn close_on_drop() -> CloseOnDrop {
    CloseOnDrop { completed: false }
}

impl CloseOnDrop {
    pub fn complete_success(mut self) {
        self.completed = true;
        complete_success(phase(schema::enums::StreamOperation::Close));
    }

    pub fn complete_error(mut self, error_type: schema::enums::ErrorType) {
        self.completed = true;
        complete_error(phase(schema::enums::StreamOperation::Close), error_type);
    }
}

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        if !self.completed
            && let Some(operation) = phase(schema::enums::StreamOperation::Close)
        {
            operation.complete(schema::enums::OutcomeValue::Cancellation, None);
        }
    }
}

#[cfg(test)]
mod tests;
