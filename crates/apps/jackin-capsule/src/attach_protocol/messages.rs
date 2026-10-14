// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

use crate::protocol::attach::{ClientTerminal, SpawnRequest};

pub(crate) const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

pub(crate) struct AttachHandshake {
    pub(crate) stream: UnixStream,
    pub(crate) peer_uid: u32,
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) spawn: Option<SpawnRequest>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) terminal: ClientTerminal,
    pub(crate) context: Option<Box<jackin_protocol::TelemetryContext>>,
    pub(crate) focus_session: Option<u64>,
    pub(crate) client_permit: tokio::sync::OwnedSemaphorePermit,
}

pub(crate) struct ControlRequest {
    pub(crate) ctx: jackin_protocol::TelemetryContext,
    pub(crate) session_capability: Option<String>,
    pub(crate) msg: jackin_protocol::control::ClientMsg,
    pub(crate) peer_uid: u32,
    pub(crate) reply: ControlReply,
}

pub(crate) enum ControlReply {
    Once(oneshot::Sender<ControlResponse>),
    Stream(mpsc::UnboundedSender<jackin_protocol::control::ServerMsg>),
}

#[derive(Debug)]
pub(crate) struct ControlResponse {
    pub(crate) msg: jackin_protocol::control::ServerMsg,
    pub(crate) operation: Option<jackin_telemetry::operation::OperationGuard>,
    pub(crate) outcome: jackin_telemetry::schema::enums::OutcomeValue,
    pub(crate) error_type: Option<jackin_telemetry::schema::enums::ErrorType>,
}

impl ControlResponse {
    pub(crate) fn complete(self, write_result: &anyhow::Result<()>) {
        if let Some(operation) = self.operation {
            operation.complete(
                if write_result.is_ok() {
                    self.outcome
                } else {
                    jackin_telemetry::schema::enums::OutcomeValue::Failure
                },
                if write_result.is_ok() {
                    self.error_type
                } else {
                    Some(RPC_ERROR)
                },
            );
        }
    }

    pub(crate) fn complete_delivery_failure(self) {
        if let Some(operation) = self.operation {
            operation.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(RPC_ERROR),
            );
        }
    }
}

#[derive(Debug)]
pub(crate) struct AttachResponseCompletion {
    pub(crate) request_id: u64,
    pub(crate) operation: Option<jackin_telemetry::operation::OperationGuard>,
    pub(crate) outcome: jackin_telemetry::schema::enums::OutcomeValue,
    pub(crate) error_type: Option<jackin_telemetry::schema::enums::ErrorType>,
}

impl AttachResponseCompletion {
    pub(crate) fn complete(self, write_result: &std::io::Result<()>) {
        if let Some(operation) = self.operation {
            operation.complete(
                if write_result.is_ok() {
                    self.outcome
                } else {
                    jackin_telemetry::schema::enums::OutcomeValue::Failure
                },
                if write_result.is_ok() {
                    self.error_type
                } else {
                    Some(RPC_ERROR)
                },
            );
        }
    }

    pub(crate) fn complete_delivery_failure(self) {
        if let Some(operation) = self.operation {
            operation.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(RPC_ERROR),
            );
        }
    }
}
