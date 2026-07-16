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
use jackin_telemetry::schema::enums::{
    ConnectionPeerType, DindMode, ErrorType, NetworkMode, OutcomeValue, WorkspaceIsolationMode,
};
use std::future::Future;

/// Emit the resolved workspace/container isolation policy without identifiers
/// or filesystem details.
pub fn isolation_decision(workspace: WorkspaceIsolationMode, network: NetworkMode, dind: DindMode) {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::WORKSPACE_ISOLATION_MODE,
            value: jackin_telemetry::Value::Str(workspace.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::NETWORK_MODE,
            value: jackin_telemetry::Value::Str(network.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::DIND_MODE,
            value: jackin_telemetry::Value::Str(dind.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(OutcomeValue::Success.as_str()),
        },
    ];
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::ISOLATION_DECISION,
        jackin_telemetry::FieldSet::new(&attrs, None),
    );
}

/// Emit the fail-closed firewall decision at the exact application failure.
pub fn isolation_firewall_failed(network: NetworkMode) {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::NETWORK_MODE,
            value: jackin_telemetry::Value::Str(network.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(OutcomeValue::Failure.as_str()),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str(ErrorType::FirewallApplyFailed.as_str()),
        },
    ];
    let _event_result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::ISOLATION_FIREWALL_FAILED,
        jackin_telemetry::FieldSet::new(&attrs, None),
    );
}

/// Run one asynchronous client connection attempt under its bounded span.
///
/// The returned transport is deliberately outside the span: callers own the
/// later handshake, control request, and close boundaries independently.
pub async fn connection_attempt<F, T>(peer: ConnectionPeerType, attempt: F) -> std::io::Result<T>
where
    F: Future<Output = std::io::Result<T>>,
{
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::CONNECTION_PEER_TYPE,
        value: jackin_telemetry::Value::Str(peer.as_str()),
    }];
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::CONNECTION_ATTEMPT,
        &attrs,
    );
    let result = attempt.await;
    complete_connection(operation, result.as_ref().err());
    result
}

/// Synchronous counterpart to [`connection_attempt`] for readiness probes.
pub fn connection_attempt_sync<T>(
    peer: ConnectionPeerType,
    attempt: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let attrs = [jackin_telemetry::Attr {
        key: jackin_telemetry::schema::attrs::CONNECTION_PEER_TYPE,
        value: jackin_telemetry::Value::Str(peer.as_str()),
    }];
    let operation = jackin_telemetry::operation_or_disabled(
        &jackin_telemetry::operation::CONNECTION_ATTEMPT,
        &attrs,
    );
    let result = attempt();
    complete_connection(operation, result.as_ref().err());
    result
}

fn complete_connection(
    operation: jackin_telemetry::operation::OperationGuard,
    error: Option<&std::io::Error>,
) {
    match error {
        None => operation.complete(OutcomeValue::Success, None),
        Some(error) => operation.complete(
            if error.kind() == std::io::ErrorKind::TimedOut {
                OutcomeValue::Timeout
            } else {
                OutcomeValue::Failure
            },
            Some(match error.kind() {
                std::io::ErrorKind::TimedOut => ErrorType::Timeout,
                std::io::ErrorKind::ConnectionRefused => ErrorType::ConnectionRefused,
                _ => ErrorType::RpcError,
            }),
        ),
    }
}

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
