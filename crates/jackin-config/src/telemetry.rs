// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_telemetry::schema::enums::{ConfigOperation, ConfigScope, ErrorType, OutcomeValue};

pub(crate) fn finish_operation<T>(
    scope: ConfigScope,
    operation: ConfigOperation,
    result: crate::ConfigResult<T>,
) -> crate::ConfigResult<T> {
    let already_owned = result
        .as_ref()
        .err()
        .is_some_and(crate::ConfigError::is_telemetry_owned);
    if !already_owned {
        let outcome = if result.is_ok() {
            OutcomeValue::Success
        } else {
            OutcomeValue::Failure
        };
        let mut attrs = vec![
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCOPE,
                value: jackin_telemetry::Value::Str(scope.as_str()),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_OPERATION,
                value: jackin_telemetry::Value::Str(operation.as_str()),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::OUTCOME,
                value: jackin_telemetry::Value::Str(outcome.as_str()),
            },
        ];
        if result.is_err() {
            attrs.push(jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
                value: jackin_telemetry::Value::Str(ErrorType::ConfigError.as_str()),
            });
        }
        let _event = jackin_telemetry::emit_event(
            &jackin_telemetry::event::CONFIG_OPERATION,
            jackin_telemetry::FieldSet::new(&attrs, None),
        );
    }
    result.map_err(crate::ConfigError::telemetry_owned)
}

#[cfg(test)]
mod tests;
