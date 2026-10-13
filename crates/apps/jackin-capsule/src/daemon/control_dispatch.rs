// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Control-request dispatch and control-server telemetry operations.

use anyhow::Result;

use crate::attach_protocol::{ControlRequest, ControlResponse};

use super::{
    Multiplexer, RPC_ERROR, control_reply_for_request, control_request_allowed,
    handle_control_subscription,
};
use jackin_protocol::control::{ClientMsg, ServerMsg};

pub(crate) fn handle_control_request(mux: &mut Multiplexer, request: ControlRequest) {
    if !control_request_allowed(
        mux,
        Some(request.peer_uid),
        request.session_capability.as_deref(),
        &request.msg,
    ) {
        let _error = jackin_telemetry::record_error(RPC_ERROR);
        match request.reply {
            crate::attach_protocol::ControlReply::Once(reply_tx) => {
                drop(reply_tx.send(ControlResponse {
                    msg: ServerMsg::Unknown,
                    operation: None,
                    outcome: jackin_telemetry::schema::enums::OutcomeValue::Failure,
                    error_type: Some(RPC_ERROR),
                }));
            }
            crate::attach_protocol::ControlReply::Stream(_) => {}
        }
        return;
    }
    let reply_tx = match request.reply {
        crate::attach_protocol::ControlReply::Stream(tx) => {
            handle_control_subscription(mux, &request.ctx, &request.msg, tx);
            return;
        }
        crate::attach_protocol::ControlReply::Once(reply_tx) => reply_tx,
    };
    let Ok(operation) = control_server_operation(&request.ctx, &request.msg) else {
        drop(reply_tx.send(ControlResponse {
            msg: ServerMsg::Unknown,
            operation: None,
            outcome: jackin_telemetry::schema::enums::OutcomeValue::Failure,
            error_type: Some(RPC_ERROR),
        }));
        return;
    };
    if let ClientMsg::ExecCommand { command, args } = request.msg {
        mux.begin_exec_picker(command, args, reply_tx, operation);
        return;
    }
    let reply = if let Some(guard) = operation.as_ref() {
        guard
            .span()
            .in_scope(|| control_reply_for_request(mux, request.msg.clone()))
    } else {
        control_reply_for_request(mux, request.msg.clone())
    };
    let unknown = matches!(reply, ServerMsg::Unknown);
    let response = ControlResponse {
        msg: reply,
        operation,
        outcome: if unknown {
            jackin_telemetry::schema::enums::OutcomeValue::Failure
        } else {
            jackin_telemetry::schema::enums::OutcomeValue::Success
        },
        error_type: unknown.then_some(RPC_ERROR),
    };
    if let Err(response) = reply_tx.send(response) {
        response.complete_delivery_failure();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlServerOperationError {
    RejectedTraceContext,
}

pub(crate) fn control_server_operation(
    context: &jackin_protocol::TelemetryContext,
    message: &ClientMsg,
) -> Result<Option<jackin_telemetry::operation::OperationGuard>, ControlServerOperationError> {
    let extracted = jackin_telemetry::propagation::extract(context);
    if matches!(
        extracted,
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        return Err(ControlServerOperationError::RejectedTraceContext);
    }
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(message.rpc_method()),
        },
    ];
    let operation = match &extracted {
        jackin_telemetry::propagation::ExtractOutcome::Parent(parent) => {
            jackin_telemetry::operation_with_remote_parent(
                &jackin_telemetry::operation::RPC_SERVER,
                &attrs,
                parent,
            )
        }
        _ => jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs),
    }
    .ok();
    Ok(operation)
}
