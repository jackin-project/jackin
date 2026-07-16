// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Typed operation facade — the structured telemetry API for jackin❯.
//!
//! Console/file tiers still render through `emit_debug_line` / `emit_compact_line`
//! (bracket prefixes live only at that render boundary). Exported OTLP bodies are
//! the clean message with attributes as dimensions — never a baked-in prefix.
//!
//! Attribute rules: low-cardinality only. Full command strings, full URLs, raw
//! payloads, and container ids are forbidden as attrs — pass redacted or
//! summarized values only. Free-text `body` is redacted before emission.
//!
//! Dynamic attributes are stamped on the current span (tracing macros require
//! static field names). Definitions come from the Weaver-validated telemetry
//! schema rather than a second runtime registry.

use crate::logging::emit_operator_notice;
use crate::redact::redact_text;
use jackin_telemetry::schema::enums::ErrorType;

/// Emit bounded diagnostic detail through the governed DEBUG event.
#[macro_export]
macro_rules! telemetry_debug {
    ($category:expr, $($arg:tt)*) => {{
        if $crate::is_debug_mode() {
            $crate::operation::telemetry_debug_line(
                $category,
                &::std::format!($($arg)*),
            );
        }
    }};
}

#[macro_export]
macro_rules! telemetry_info {
    ($category:expr, $($arg:tt)*) => {{
        $crate::operation::telemetry_line(
            $crate::OperationLevel::Info,
            $category,
            &::std::format!($($arg)*),
        );
    }};
}

#[macro_export]
macro_rules! telemetry_warn {
    ($category:expr, $($arg:tt)*) => {{
        $crate::operation::telemetry_line(
            $crate::OperationLevel::Warn,
            $category,
            &::std::format!($($arg)*),
        );
    }};
}

#[macro_export]
macro_rules! telemetry_error {
    ($error_type:expr, $($arg:tt)*) => {{
        $crate::operation::telemetry_error_line(
            $error_type,
            &::std::format!($($arg)*),
        );
    }};
}

#[doc(hidden)]
pub fn telemetry_debug_line(category: &'static str, body: &str) {
    telemetry_line(OperationLevel::Debug, category, body);
}

#[doc(hidden)]
pub fn telemetry_line(level: OperationLevel, category: &'static str, body: &str) {
    let _ = category;
    let body = redact_text(body);
    let (def, outcome) = match level {
        OperationLevel::Info => (&jackin_telemetry::event::OPERATION_LOG, "success"),
        OperationLevel::Debug => (&jackin_telemetry::event::DEBUG_LINE, "success"),
        OperationLevel::Warn => (&jackin_telemetry::event::OPERATION_WARN, "cancellation"),
        OperationLevel::Error => (&jackin_telemetry::event::ERROR_TYPED, "failure"),
    };
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::OUTCOME,
        value: jackin_telemetry::Value::Str(outcome),
    }];
    let _event_result = jackin_telemetry::emit_event(
        def,
        jackin_telemetry::FieldSet::new(&attrs, Some(body.as_ref())),
    );
}

#[doc(hidden)]
pub fn telemetry_error_line(error_type: ErrorType, body: &str) {
    let body = redact_text(body);
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str("failure"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str(error_type.as_str()),
        },
    ];
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::ERROR_TYPED,
        jackin_telemetry::FieldSet::new(&attrs, Some(body.as_ref())),
    );
    emit_operator_notice(body.as_ref());
}

/// Severity for [`operation_log`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationLevel {
    Info,
    Debug,
    Warn,
    Error,
}
