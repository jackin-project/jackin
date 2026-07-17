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
