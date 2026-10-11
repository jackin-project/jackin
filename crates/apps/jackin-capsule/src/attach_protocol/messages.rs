// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Attach/control message types and completion guards.

use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

use crate::protocol::attach::{ClientTerminal, SpawnRequest};

pub(crate) const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

/// A validated attach handshake produced by `perform_handshake`. The
/// main loop applies these — `client_permit` is kept alive until the
/// spawned persistent attach task drops it.
pub(crate) struct AttachHandshake {
    pub(crate) stream: UnixStream,
    /// Kernel-authenticated peer identity captured before the handshake is
    /// forwarded to the daemon loop. Session UIDs are never attach clients.
    pub(crate) peer_uid: u32,
    pub(crate) rows: u16,
    pub(crate) cols: u16,
    pub(crate) spawn: Option<SpawnRequest>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) terminal: ClientTerminal,
    pub(crate) context: Option<Box<jackin_protocol::TelemetryContext>>,
    /// `Some(session_id)` when the client (typically the host
    /// console picking out of the snapshot preview) wants the daemon
    /// to focus a specific pane before forwarding content. The main
    /// loop calls `Multiplexer::focus_session_globally` on receipt.
    /// Unknown ids are silently ignored — see the daemon arm.
    pub(crate) focus_session: Option<u64>,
    pub(crate) client_permit: tokio::sync::OwnedSemaphorePermit,
}

pub(crate) struct ControlRequest {
    pub(crate) ctx: jackin_protocol::TelemetryContext,
    /// Daemon-issued session capability copied from the wire envelope. It is
    /// checked with the kernel peer UID before target-scoped dispatch.
    pub(crate) session_capability: Option<String>,
    pub(crate) msg: jackin_protocol::control::ClientMsg,
    /// Kernel-authenticated peer identity. The wire request remains unchanged;
    /// this metadata is added only after the daemon accepts the socket.
    pub(crate) peer_uid: u32,
    pub(crate) reply: ControlReply,
}

/// Where the daemon writes the answer to a control request.
///
/// Almost every control RPC is one framed reply followed by a close, so the
/// channel is a `oneshot`. `events` is a subscription: the daemon keeps
/// writing frames for as long as the peer reads them, so it needs a stream
/// sender the multiplexer can hold on to. Making the difference a sum type
/// means a subscription can never be answered with a single frame (nor a
/// query left hanging on a stream that never closes).
pub(crate) enum ControlReply {
    /// One framed reply, then the connection closes.
    Once(oneshot::Sender<ControlResponse>),
    /// A long-lived subscription. The daemon holds the sender; dropping the
    /// receiver (the peer hung up) is what unsubscribes.
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
