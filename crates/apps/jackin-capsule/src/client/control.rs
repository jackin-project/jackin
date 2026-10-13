// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Control socket transport: connect, request, and framed replies.

use anyhow::{Context, Result};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::protocol::control::{ClientMsg, ControlRequest, ServerMsg, frame as control_frame};
use crate::socket::SOCKET_PATH;

/// Connect to the daemon control socket and send one length-prefixed request,
/// returning the open stream so the caller can read (or ignore) the reply.
pub(crate) async fn connect_and_send(
    request: &ClientMsg,
) -> Result<(
    UnixStream,
    Option<jackin_telemetry::operation::OperationGuard>,
)> {
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str(request.rpc_method()),
        },
    ];
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_CLIENT, &attrs).ok();
    let mut stream = match jackin_diagnostics::operation::connection_attempt(
        jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleControl,
        UnixStream::connect(SOCKET_PATH),
    )
    .await
    {
        Ok(stream) => stream,
        Err(error) => {
            if let Some(operation) = operation {
                operation.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Failure,
                    Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
                );
            }
            return Err(error).context("cannot connect to jackin-capsule daemon");
        }
    };
    let mut ctx = jackin_protocol::TelemetryContext::v1();
    if let Some(operation) = operation.as_ref() {
        operation
            .span()
            .in_scope(|| jackin_telemetry::propagation::inject(&mut ctx));
    } else {
        jackin_telemetry::propagation::inject(&mut ctx);
    }
    let result = async {
        jackin_protocol::capsule_transport::client_handshake_async(&mut stream)
            .await
            .context("negotiating Capsule control transport")?;
        stream
            .write_all(&control_frame(&ControlRequest {
                ctx,
                session_capability: std::env::var(jackin_protocol::SESSION_CAPABILITY_ENV)
                    .ok()
                    .filter(|value| !value.is_empty()),
                msg: request.clone(),
            }))
            .await
            .context("writing Capsule control request")
    }
    .await;
    if let Err(error) = result {
        if let Some(operation) = operation {
            operation.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
            );
        }
        return Err(error);
    }
    Ok((stream, operation))
}

pub(crate) async fn request_control(request: &ClientMsg) -> Result<ServerMsg> {
    let (mut stream, operation) = connect_and_send(request).await?;
    let result = read_control_reply(&mut stream).await.and_then(|reply| {
        anyhow::ensure!(
            control_response_matches(request, &reply),
            "daemon replied with {} for {} request",
            reply.kind(),
            request.rpc_method()
        );
        Ok(reply)
    });
    if let Some(operation) = operation {
        operation.complete(
            if result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            result
                .as_ref()
                .err()
                .map(|_| jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
    result
}

pub(crate) fn control_response_matches(request: &ClientMsg, response: &ServerMsg) -> bool {
    matches!(
        (request, response),
        (
            ClientMsg::TelemetryHealth,
            ServerMsg::TelemetryHealth { .. }
        ) | (ClientMsg::Status, ServerMsg::SessionList { .. })
            | (ClientMsg::Snapshot, ServerMsg::Snapshot { .. })
            | (ClientMsg::Agents, ServerMsg::AgentRegistry { .. })
            | (
                ClientMsg::ReportRuntimeEvent { .. } | ClientMsg::StatusCapture { .. },
                ServerMsg::Ack,
            )
            | (ClientMsg::UsageFocused, ServerMsg::UsageFocused { .. })
            | (
                ClientMsg::UsageRefreshFocused,
                ServerMsg::UsageFocused { .. }
            )
            | (ClientMsg::UsageAccountList, ServerMsg::UsageAccounts { .. })
            | (ClientMsg::ExecCommand { .. }, ServerMsg::ExecResult { .. })
            | (ClientMsg::ExecCommand { .. }, ServerMsg::ExecDenied { .. })
            | (ClientMsg::TokenUsage { .. }, ServerMsg::TokenUsage { .. })
            | (
                ClientMsg::SessionSend { .. },
                ServerMsg::SessionSent { .. } | ServerMsg::SessionSendDenied { .. },
            )
    )
}

/// Read one framed reply, or `None` when the peer closed the connection at a
/// frame boundary. Only the subscription reader needs the distinction: for a
/// one-shot RPC a missing reply is always an error.
pub(crate) async fn read_control_reply_or_eof(
    stream: &mut UnixStream,
) -> Result<Option<ServerMsg>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    read_control_body(stream, len_buf).await.map(Some)
}

pub(crate) async fn read_control_reply(stream: &mut UnixStream) -> Result<ServerMsg> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    read_control_body(stream, len_buf).await
}

pub(crate) async fn read_control_body(
    stream: &mut UnixStream,
    len_buf: [u8; 4],
) -> Result<ServerMsg> {
    let len = u32::from_be_bytes(len_buf) as usize;
    // Mirror the daemon-side cap in `socket::read_control_msg`.
    pub(crate) const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_REPLY {
        anyhow::bail!("daemon control reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}
