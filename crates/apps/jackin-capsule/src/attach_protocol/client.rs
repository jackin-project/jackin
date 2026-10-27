// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{AttachResponseCompletion, RPC_ERROR, detach_attached_task};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::daemon::Multiplexer;
use crate::protocol::attach::{ClientFrame, read_client_frame};

pub(crate) async fn detach_client(mux: &mut Multiplexer) {
    detach_attached_task(mux, "detach_client").await;
}

pub(crate) async fn handle_attach_client_with_handshake(
    mut stream: UnixStream,
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut completion_rx: mpsc::UnboundedReceiver<AttachResponseCompletion>,
    cmd_tx: mpsc::UnboundedSender<ClientFrame>,
    mut handshake_operation: Option<jackin_telemetry::operation::OperationGuard>,
) {
    let open =
        jackin_telemetry::stream::phase(jackin_telemetry::schema::enums::StreamOperation::Open);
    jackin_telemetry::stream::complete_success(open);
    let close = jackin_telemetry::stream::close_on_drop();
    let mut completions = std::collections::HashMap::new();
    let mut tag = [0u8; 1];
    let mut terminal_error = None;
    let mut cancelled = false;
    loop {
        tokio::select! {
            biased;
            Some(completion) = completion_rx.recv() => {
                completions.insert(completion.request_id, completion);
            }
            result = stream.read_exact(&mut tag) => {
                if let Err(e) = result {
                    if e.kind() != std::io::ErrorKind::UnexpectedEof {
                        terminal_error = Some(RPC_ERROR);
                    }
                    break;
                }
                let frame = match read_client_frame(&mut stream, tag[0]).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        terminal_error = Some(RPC_ERROR);
                        break;
                    }
                    Err(_) => {
                        terminal_error = Some(RPC_ERROR);
                        break;
                    }
                };
                if matches!(
                    frame,
                    ClientFrame::Detach
                        | ClientFrame::FocusIn
                        | ClientFrame::FocusOut
                        | ClientFrame::ClipboardImage(_)
                        | ClientFrame::ClipboardImageStart(_)
                        | ClientFrame::ClipboardImageChunk(_)
                        | ClientFrame::ClipboardImageEnd(_)
                        | ClientFrame::ClipboardImageError(_)
                ) {
                    let _warning = jackin_telemetry::record_recovered_degradation();
                    continue;
                }
                if cmd_tx.send(frame).is_err() {
                    cancelled = true;
                    break;
                }
            }
            Some(bytes) = out_rx.recv() => {
                let write_result = stream.write_all(&bytes).await;
                let mut response_owned_error = false;
                if bytes.first() == Some(&jackin_protocol::attach::TAG_ATTACH_CONTROL_RESPONSE)
                    && bytes.len() >= 13
                {
                    let request_id = u64::from_be_bytes(
                        bytes[5..13].try_into().unwrap_or_default(),
                    );
                    if let Some(completion) = completions.remove(&request_id) {
                        response_owned_error = write_result.is_err();
                        completion.complete(&write_result);
                    }
                }
                let handshake_owned_error = handshake_operation.is_some() && write_result.is_err();
                if let Some(operation) = handshake_operation.take() {
                    operation.complete(
                        if write_result.is_ok() {
                            jackin_telemetry::schema::enums::OutcomeValue::Success
                        } else {
                            jackin_telemetry::schema::enums::OutcomeValue::Failure
                        },
                        write_result.as_ref().err().map(|_| RPC_ERROR),
                    );
                }
                if let Err(e) = write_result {
                    if !matches!(
                        e.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe
                    ) && !handshake_owned_error
                        && !response_owned_error
                    {
                        terminal_error = Some(RPC_ERROR);
                    }
                    break;
                }
            }
        }
    }
    if let Some(operation) = handshake_operation {
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(RPC_ERROR),
        );
    }
    for (_, completion) in completions {
        completion.complete_delivery_failure();
    }
    drop(cmd_tx.send(ClientFrame::Detach));
    match terminal_error {
        Some(error) => close.complete_error(error),
        None if cancelled => drop(close),
        None => close.complete_success(),
    }
}
