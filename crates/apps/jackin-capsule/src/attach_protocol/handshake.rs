// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AttachHandshake, ControlReply, ControlRequest};
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;

use crate::protocol::attach::{ClientFrame, read_client_frame};
use crate::socket;

pub(crate) async fn perform_handshake(
    mut stream: UnixStream,
    client_permit: tokio::sync::OwnedSemaphorePermit,
    handshake_tx: mpsc::UnboundedSender<AttachHandshake>,
    control_tx: mpsc::UnboundedSender<ControlRequest>,
) -> jackin_telemetry::spawn::DetachedCompletion {
    const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

    let peer_uid = if let Ok(credentials) = stream.peer_cred() {
        credentials.uid()
    } else {
        drop(client_permit);
        return jackin_telemetry::spawn::DetachedCompletion::failure(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        );
    };

    match tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        jackin_protocol::capsule_transport::server_handshake_async(&mut stream),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(_)) => {
            drop(client_permit);
            return jackin_telemetry::spawn::DetachedCompletion::failure(
                jackin_telemetry::schema::enums::ErrorType::RpcError,
            );
        }
        Err(_) => {
            drop(client_permit);
            return jackin_telemetry::spawn::DetachedCompletion::timeout();
        }
    }

    let mut first = [0u8; 1];
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, stream.read(&mut first)).await {
        Ok(Ok(0)) => return jackin_telemetry::spawn::DetachedCompletion::success(),
        Ok(Ok(_)) => {}
        Ok(Err(_)) => {
            drop(client_permit);
            return jackin_telemetry::spawn::DetachedCompletion::error(
                jackin_telemetry::schema::enums::ErrorType::RpcError,
            );
        }
        Err(_) => {
            drop(client_permit);
            return jackin_telemetry::spawn::DetachedCompletion::timeout();
        }
    }
    if first[0] == 0x00 {
        return perform_control_handshake(
            stream,
            first[0],
            client_permit,
            peer_uid,
            control_tx,
            HANDSHAKE_TIMEOUT,
        )
        .await;
    }
    let initial_frame =
        match tokio::time::timeout(HANDSHAKE_TIMEOUT, read_client_frame(&mut stream, first[0]))
            .await
        {
            Ok(Ok(Some(frame))) => frame,
            Ok(Ok(None)) => {
                drop(client_permit);
                return jackin_telemetry::spawn::DetachedCompletion::failure(
                    jackin_telemetry::schema::enums::ErrorType::RpcError,
                );
            }
            Ok(Err(_)) => {
                drop(client_permit);
                return jackin_telemetry::spawn::DetachedCompletion::error(
                    jackin_telemetry::schema::enums::ErrorType::RpcError,
                );
            }
            Err(_) => {
                drop(client_permit);
                return jackin_telemetry::spawn::DetachedCompletion::timeout();
            }
        };
    let ClientFrame::Hello {
        rows,
        cols,
        spawn,
        env,
        terminal,
        context,
        focus_session,
    } = initial_frame
    else {
        drop(client_permit);
        return jackin_telemetry::spawn::DetachedCompletion::failure(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        );
    };
    if context.as_ref().is_some_and(|ctx| {
        matches!(
            jackin_telemetry::propagation::extract(ctx.as_ref()),
            jackin_telemetry::propagation::ExtractOutcome::RejectRequest
        )
    }) {
        drop(client_permit);
        return jackin_telemetry::spawn::DetachedCompletion::failure(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        );
    }
    let handshake = AttachHandshake {
        stream,
        peer_uid,
        rows,
        cols,
        spawn,
        env,
        terminal,
        context,
        focus_session,
        client_permit,
    };
    if handshake_tx.send(handshake).is_err() {
        return jackin_telemetry::spawn::DetachedCompletion::error(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        );
    }
    jackin_telemetry::spawn::DetachedCompletion::success()
}

pub(crate) async fn perform_control_handshake(
    mut stream: UnixStream,
    first_tag: u8,
    client_permit: tokio::sync::OwnedSemaphorePermit,
    peer_uid: u32,
    control_tx: mpsc::UnboundedSender<ControlRequest>,
    timeout: Duration,
) -> jackin_telemetry::spawn::DetachedCompletion {
    let Ok(request) = socket::read_control_msg(&mut stream, first_tag).await else {
        return jackin_telemetry::spawn::DetachedCompletion::failure(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        );
    };
    if request.msg.is_subscription() {
        let completion = serve_control_subscription(
            stream,
            request.ctx,
            request.session_capability,
            request.msg,
            peer_uid,
            control_tx,
        )
        .await;
        drop(client_permit);
        return completion;
    }
    let (reply_tx, reply_rx) = oneshot::channel();
    if control_tx
        .send(ControlRequest {
            ctx: request.ctx,
            session_capability: request.session_capability,
            msg: request.msg,
            peer_uid,
            reply: ControlReply::Once(reply_tx),
        })
        .is_err()
    {
        return jackin_telemetry::spawn::DetachedCompletion::error(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        );
    }
    let completion = match tokio::time::timeout(timeout, reply_rx).await {
        Ok(Ok(response)) => {
            let write_result = socket::write_control_reply(stream, &response.msg).await;
            let completion = if write_result.is_ok() {
                jackin_telemetry::spawn::DetachedCompletion::success()
            } else {
                jackin_telemetry::spawn::DetachedCompletion::error(
                    jackin_telemetry::schema::enums::ErrorType::RpcError,
                )
            };
            response.complete(&write_result);
            completion
        }
        Ok(Err(_)) => jackin_telemetry::spawn::DetachedCompletion::error(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        ),
        Err(_) => jackin_telemetry::spawn::DetachedCompletion::timeout(),
    };
    drop(client_permit);
    completion
}

pub(crate) async fn serve_control_subscription(
    mut stream: UnixStream,
    ctx: jackin_protocol::TelemetryContext,
    session_capability: Option<String>,
    msg: jackin_protocol::control::ClientMsg,
    peer_uid: u32,
    control_tx: mpsc::UnboundedSender<ControlRequest>,
) -> jackin_telemetry::spawn::DetachedCompletion {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    if control_tx
        .send(ControlRequest {
            ctx,
            session_capability,
            msg,
            peer_uid,
            reply: ControlReply::Stream(event_tx),
        })
        .is_err()
    {
        return jackin_telemetry::spawn::DetachedCompletion::error(
            jackin_telemetry::schema::enums::ErrorType::RpcError,
        );
    }
    let mut discard = [0u8; 64];
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                let Some(event) = event else {
                    return jackin_telemetry::spawn::DetachedCompletion::success();
                };
                if socket::write_control_frame(&mut stream, &event).await.is_err() {
                    return jackin_telemetry::spawn::DetachedCompletion::error(
                        jackin_telemetry::schema::enums::ErrorType::RpcError,
                    );
                }
            }
            read = stream.read(&mut discard) => {
                return match read {
                    Ok(0) => jackin_telemetry::spawn::DetachedCompletion::success(),
                    _ => jackin_telemetry::spawn::DetachedCompletion::failure(
                        jackin_telemetry::schema::enums::ErrorType::RpcError,
                    ),
                };
            }
        }
    }
}
