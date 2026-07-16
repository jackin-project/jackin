// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Deferred control replies whose telemetry ends only after response delivery.

use std::future::Future;

use jackin_protocol::control::ServerMsg;

use crate::attach_protocol::ControlResponse;

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

pub(crate) struct PendingExecReply {
    reply_tx: tokio::sync::oneshot::Sender<ControlResponse>,
    operation: Option<jackin_telemetry::operation::OperationGuard>,
}

impl PendingExecReply {
    pub(super) fn new(
        reply_tx: tokio::sync::oneshot::Sender<ControlResponse>,
        operation: Option<jackin_telemetry::operation::OperationGuard>,
    ) -> Self {
        Self {
            reply_tx,
            operation,
        }
    }

    pub(super) fn spawn<F>(self, future: F)
    where
        F: Future<Output = ServerMsg> + Send + 'static,
    {
        let server_span = self
            .operation
            .as_ref()
            .map(|operation| operation.span().clone());
        let spawn = move || {
            jackin_telemetry::spawn::spawn_detached_with_completion(
                &jackin_telemetry::operation::PROCESS_COMMAND,
                async move {
                    let reply = future.await;
                    let (outcome, error_type) = process_exec_reply_outcome(&reply);
                    self.send_with_outcome(reply, outcome, error_type);
                    jackin_telemetry::spawn::DetachedCompletion {
                        outcome,
                        error_type,
                    }
                },
            )
        };
        if let Some(server_span) = server_span {
            drop(server_span.in_scope(spawn));
        } else {
            drop(spawn());
        }
    }

    pub(super) fn send(self, reply: ServerMsg) {
        let (outcome, error_type) = exec_reply_outcome(&reply);
        self.send_with_outcome(reply, outcome, error_type);
    }

    fn send_with_outcome(
        self,
        reply: ServerMsg,
        outcome: jackin_telemetry::schema::enums::OutcomeValue,
        error_type: Option<jackin_telemetry::schema::enums::ErrorType>,
    ) {
        let response = ControlResponse {
            msg: reply,
            operation: self.operation,
            outcome,
            error_type,
        };
        if let Err(response) = self.reply_tx.send(response) {
            response.complete_delivery_failure();
        }
    }
}

fn exec_reply_outcome(
    reply: &ServerMsg,
) -> (
    jackin_telemetry::schema::enums::OutcomeValue,
    Option<jackin_telemetry::schema::enums::ErrorType>,
) {
    match reply {
        ServerMsg::ExecResult { exit_code: 0, .. } => {
            (jackin_telemetry::schema::enums::OutcomeValue::Success, None)
        }
        ServerMsg::ExecResult { .. } => (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        ),
        ServerMsg::ExecDenied { .. } => (
            jackin_telemetry::schema::enums::OutcomeValue::Cancellation,
            None,
        ),
        _ => (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        ),
    }
}

fn process_exec_reply_outcome(
    reply: &ServerMsg,
) -> (
    jackin_telemetry::schema::enums::OutcomeValue,
    Option<jackin_telemetry::schema::enums::ErrorType>,
) {
    match reply {
        ServerMsg::ExecResult { exit_code: 0, .. } => {
            (jackin_telemetry::schema::enums::OutcomeValue::Success, None)
        }
        ServerMsg::ExecResult { .. } | ServerMsg::ExecDenied { .. } => (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        ),
        _ => (
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        ),
    }
}
