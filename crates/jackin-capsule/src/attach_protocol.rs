// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Client attach/detach lifecycle for the capsule multiplexer.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;

use crate::daemon::Multiplexer;
use crate::protocol::attach::{
    ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_server, read_client_frame,
};
use crate::socket;

const RPC_ERROR: jackin_telemetry::schema::enums::ErrorType =
    jackin_telemetry::schema::enums::ErrorType::RpcError;

fn record_attach_failure(body: &'static str) {
    jackin_diagnostics::operation::telemetry_error_line(
        jackin_telemetry::schema::enums::ErrorType::RpcError,
        body,
    );
}

/// A validated attach handshake produced by `perform_handshake`. The
/// main loop applies these — `client_permit` is kept alive until the
/// spawned persistent attach task drops it.
pub(crate) struct AttachHandshake {
    pub(crate) stream: UnixStream,
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
    pub(crate) msg: jackin_protocol::control::ClientMsg,
    pub(crate) reply_tx: oneshot::Sender<ControlResponse>,
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
    fn complete(self, write_result: &std::io::Result<()>) {
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

/// Per-connection handshake task. Reads the first byte, routes
/// control-channel requests back to the main daemon loop (one-shot
/// reply, closes the socket), and forwards validated attach Hellos
/// back to the main loop via `handshake_tx`. Owning the slow
/// `read_exact` here keeps a silent or slow client from stalling the
/// daemon's main `select!`.
pub(crate) async fn perform_handshake(
    mut stream: UnixStream,
    client_permit: tokio::sync::OwnedSemaphorePermit,
    handshake_tx: mpsc::UnboundedSender<AttachHandshake>,
    control_tx: mpsc::UnboundedSender<ControlRequest>,
) {
    // Bound the handshake reads. A client that opens the socket and
    // never sends a byte otherwise holds the `OwnedSemaphorePermit`
    // forever — sixteen silent peers would starve the
    // `MAX_CONCURRENT_CLIENTS` cap and lock out legitimate attaches.
    const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

    let mut first = [0u8; 1];
    match tokio::time::timeout(HANDSHAKE_TIMEOUT, stream.read_exact(&mut first)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "attach: handshake read_exact(first byte) failed: {e}"
            );
            drop(client_permit);
            return;
        }
        Err(_) => {
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "attach: handshake first byte not received within {HANDSHAKE_TIMEOUT:?}; dropping connection"
            );
            drop(client_permit);
            return;
        }
    }
    if first[0] == 0x00 {
        perform_control_handshake(
            stream,
            first[0],
            client_permit,
            control_tx,
            HANDSHAKE_TIMEOUT,
        )
        .await;
        return;
    }
    let initial_frame = match tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        read_client_frame(&mut stream, first[0]),
    )
    .await
    {
        Ok(Ok(Some(frame))) => frame,
        Ok(Ok(None)) => {
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "attach: handshake EOF before initial frame"
            );
            drop(client_permit);
            return;
        }
        Ok(Err(e)) => {
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "attach: handshake frame decode failed: {e}"
            );
            drop(client_permit);
            return;
        }
        Err(_) => {
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "attach: handshake Hello frame not received within {HANDSHAKE_TIMEOUT:?}; dropping connection"
            );
            drop(client_permit);
            return;
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
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "attach: rejected client whose first frame was not Hello: {initial_frame:?}"
        );
        drop(client_permit);
        return;
    };
    if context.as_ref().is_some_and(|ctx| {
        matches!(
            jackin_telemetry::propagation::extract(ctx.as_ref()),
            jackin_telemetry::propagation::ExtractOutcome::RejectRequest
        )
    }) {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "attach: rejected invalid telemetry correlation"
        );
        drop(client_permit);
        return;
    }
    let handshake = AttachHandshake {
        stream,
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
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "attach: handshake channel closed; daemon shutting down"
        );
    }
}

async fn perform_control_handshake(
    mut stream: UnixStream,
    first_tag: u8,
    client_permit: tokio::sync::OwnedSemaphorePermit,
    control_tx: mpsc::UnboundedSender<ControlRequest>,
    timeout: Duration,
) {
    let request = match socket::read_control_msg(&mut stream, first_tag).await {
        Ok(request) => request,
        Err(error) => {
            jackin_diagnostics::telemetry_info!(
                "capsule",
                "control: rejecting malformed request: {error:#}"
            );
            return;
        }
    };
    let (reply_tx, reply_rx) = oneshot::channel();
    if control_tx
        .send(ControlRequest {
            ctx: request.ctx,
            msg: request.msg,
            reply_tx,
        })
        .is_err()
    {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "control: daemon loop unavailable while handling request"
        );
        return;
    }
    match tokio::time::timeout(timeout, reply_rx).await {
        Ok(Ok(response)) => {
            let write_result = socket::write_control_reply(stream, &response.msg).await;
            if let Err(error) = &write_result {
                jackin_diagnostics::telemetry_info!(
                    "capsule",
                    "control reply delivery failed: {error:#}"
                );
            }
            response.complete(&write_result);
        }
        Ok(Err(_)) => jackin_diagnostics::telemetry_info!(
            "capsule",
            "control: reply channel closed before response"
        ),
        Err(_) => jackin_diagnostics::telemetry_info!(
            "capsule",
            "control: daemon reply timed out after {timeout:?}"
        ),
    }
    drop(client_permit);
}

pub(crate) async fn drain_and_exit(mux: &mut Multiplexer) {
    drain_and_exit_with_reason(mux, None).await;
}

pub(crate) async fn drain_and_exit_with_reason(mux: &mut Multiplexer, reason: Option<String>) {
    if let Some(reason) = reason.as_deref() {
        mux.send_out_of_band(format!("\r\n[jackin-capsule] {reason}\r\n").into_bytes());
    }
    gracefully_detach_attached_task_with_reason(mux, "drain_and_exit", reason.as_deref()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
}

const ATTACH_SHUTDOWN_FLUSH_GRACE_MS: u64 = 50;
const ATTACH_SHUTDOWN_CLOSE_GRACE_MS: u64 = 1000;

pub(crate) fn send_attached_shutdown(
    mux: &mut Multiplexer,
    context: &str,
    reason: Option<&str>,
) -> bool {
    mux.client_registry.client.flush_out_of_band();
    let Some(tx) = mux.client_registry.client.take() else {
        return false;
    };
    if tx
        .send(encode_server(ServerFrame::Shutdown {
            reason: reason.map(str::to_owned),
        }))
        .is_err()
    {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "{context}: client receiver already dropped; Shutdown frame not delivered"
        );
    }
    true
}

/// Centralised detach for the currently-attached client. Take-then-
/// send-then-wait-then-abort, in that order, so a takeover/cancel race never
/// leaves `attached_task = Some` with a dead `attached_out`: take the
/// out-channel sender first (so the next frame queue allocation does
/// not race with the old receiver), send Shutdown best-effort, give
/// the attach task a brief writer-side drain window, then
/// abort the attach task so its reader stops pushing into the shared
/// `cmd_tx`. Used by SIGTERM / SIGINT shutdown, explicit detach, and
/// `drain_and_exit`.
pub(crate) async fn detach_attached_task(mux: &mut Multiplexer, context: &str) {
    detach_attached_task_with_reason(mux, context, None).await;
}

async fn detach_attached_task_with_reason(
    mux: &mut Multiplexer,
    context: &str,
    reason: Option<&str>,
) {
    let had_sender = send_attached_shutdown(mux, context, reason);
    // The latch is paired with the sender's lifetime: clearing
    // `attached_out` invalidates the previous attach, so the next
    // assignment (in the takeover branch of `run_daemon`) starts from
    // a clean state regardless of which code path reassigns it.
    if had_sender {
        tokio::time::sleep(Duration::from_millis(ATTACH_SHUTDOWN_FLUSH_GRACE_MS)).await;
    }
    if let Some(handle) = mux.client_registry.attached_task.take() {
        handle.abort();
    }
}

async fn gracefully_detach_attached_task_with_reason(
    mux: &mut Multiplexer,
    context: &str,
    reason: Option<&str>,
) {
    let had_sender = send_attached_shutdown(mux, context, reason);
    let Some(mut handle) = mux.client_registry.attached_task.take() else {
        return;
    };
    if !had_sender {
        handle.abort();
        return;
    }
    tokio::select! {
        result = &mut handle => {
            if let Err(err) = result
                && !err.is_cancelled()
            {
                jackin_diagnostics::telemetry_info!("capsule", "{context}: attach task ended with join error: {err}");
            }
        }
        () = tokio::time::sleep(Duration::from_millis(ATTACH_SHUTDOWN_CLOSE_GRACE_MS)) => {
            jackin_diagnostics::telemetry_info!("capsule",
                "{context}: attach client did not close after Shutdown within {ATTACH_SHUTDOWN_CLOSE_GRACE_MS}ms; aborting"
            );
            handle.abort();
        }
    }
}

pub(crate) fn initial_spawn_request(
    initial_agent: &str,
    initial_provider: Option<&jackin_protocol::InitialProvider>,
) -> SpawnRequest {
    if initial_agent.is_empty() {
        SpawnRequest::Shell
    } else if let Some(provider) = initial_provider {
        SpawnRequest::AgentWithProvider {
            slug: initial_agent.to_owned(),
            provider_label: provider.label.clone(),
        }
    } else {
        SpawnRequest::Agent(initial_agent.to_owned())
    }
}

pub(crate) fn spawn_request_label(request: &SpawnRequest) -> String {
    match request {
        SpawnRequest::Agent(agent) => format!("agent {agent:?}"),
        SpawnRequest::AgentWithProvider {
            slug,
            provider_label,
        } => {
            format!("agent {slug:?} (provider: {provider_label})")
        }
        SpawnRequest::Shell => "shell".to_owned(),
    }
}

pub(crate) async fn detach_client(mux: &mut Multiplexer) {
    detach_attached_task(mux, "detach_client").await;
}

/// Per-client connection handler: bidirectional bridge between the
/// socket and the main daemon loop. Reads `ClientFrame`s off the
/// socket and pushes them through `cmd_tx`; writes any bytes
/// received on `out_rx` back to the socket. Exits on any I/O error
/// or when either channel closes (which happens during takeover —
/// `attached_task.abort()` ends this task before its socket sees EOF).
#[cfg(test)]
pub(crate) async fn handle_attach_client(
    stream: UnixStream,
    out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cmd_tx: mpsc::UnboundedSender<ClientFrame>,
) {
    let (_completion_tx, completion_rx) = mpsc::unbounded_channel();
    handle_attach_client_with_handshake(stream, out_rx, completion_rx, cmd_tx, None).await;
}

pub(crate) async fn handle_attach_client_with_handshake(
    mut stream: UnixStream,
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    mut completion_rx: mpsc::UnboundedReceiver<AttachResponseCompletion>,
    cmd_tx: mpsc::UnboundedSender<ClientFrame>,
    mut handshake_operation: Option<jackin_telemetry::operation::OperationGuard>,
) {
    let mut completions = std::collections::HashMap::new();
    let mut tag = [0u8; 1];
    loop {
        tokio::select! {
            biased;
            Some(completion) = completion_rx.recv() => {
                completions.insert(completion.request_id, completion);
            }
            result = stream.read_exact(&mut tag) => {
                if let Err(e) = result {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        // Expected detach — not a failure (plan 008).
                        jackin_diagnostics::telemetry_debug!("capsule", "attach client: socket closed (client detached)");
                    } else {
                        // Operator-visible breadcrumb + typed OTLP failure.
                        jackin_diagnostics::telemetry_error!(jackin_telemetry::schema::enums::ErrorType::RpcError, "attach client: socket read failed: {e}");
                        record_attach_failure("attach socket read failed");
                    }
                    break;
                }
                let frame = match read_client_frame(&mut stream, tag[0]).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        jackin_diagnostics::telemetry_warn!("capsule", "attach client: EOF mid-frame (tag={:#04x})", tag[0]);
                        record_attach_failure("attach socket closed mid-frame");
                        break;
                    }
                    Err(e) => {
                        jackin_diagnostics::telemetry_error!(jackin_telemetry::schema::enums::ErrorType::RpcError,
                            "attach client: frame decode failed (tag={:#04x}): {e}",
                            tag[0]
                        );
                        record_attach_failure("attach frame decode failed");
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
                    jackin_diagnostics::telemetry_warn!(
                        "capsule",
                        "attach client: rejected uncontextual legacy control frame"
                    );
                    continue;
                }
                if cmd_tx.send(frame).is_err() {
                    jackin_diagnostics::telemetry_info!("capsule", "attach client: cmd_tx closed; daemon shutting down");
                    return;
                }
            }
            Some(bytes) = out_rx.recv() => {
                let write_result = stream.write_all(&bytes).await;
                if bytes.first() == Some(&jackin_protocol::attach::TAG_ATTACH_CONTROL_RESPONSE)
                    && bytes.len() >= 13
                {
                    let request_id = u64::from_be_bytes(
                        bytes[5..13].try_into().unwrap_or_default(),
                    );
                    if let Some(completion) = completions.remove(&request_id) {
                        completion.complete(&write_result);
                    }
                }
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
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe
                    ) {
                        jackin_diagnostics::telemetry_warn!("capsule", "attach client: socket write failed: {e}");
                    } else {
                        jackin_diagnostics::telemetry_error!(jackin_telemetry::schema::enums::ErrorType::RpcError, "attach client: socket write failed: {e}");
                        record_attach_failure("attach socket write failed");
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
    // Signal the main loop that this client is gone so it can clear
    // `attached_out` / `attached_task` — without this, subsequent
    // `send_to_client` calls silently drop into the closed channel
    // and the daemon keeps treating the dead socket as live. If the
    // main loop is already shutting down the send fails; log so the
    // exact symptom this comment warns against does not happen
    // silently if the cmd_tx side is the one that died first.
    if cmd_tx.send(ClientFrame::Detach).is_err() {
        jackin_diagnostics::telemetry_info!(
            "capsule",
            "attach client: cmd_tx closed before synthetic Detach could fire; main loop is already tearing down"
        );
    }
}

#[cfg(test)]
mod tests;
