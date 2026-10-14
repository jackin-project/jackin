// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_telemetry::schema::enums::{
    ConnectionPeerType, DindMode, ErrorType, NetworkMode, OutcomeValue, WorkspaceIsolationMode,
};
use std::future::Future;

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
