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

fn record_attach_failure(error_type: &'static str, body: &'static str) {
    let span = jackin_diagnostics::operation_span("capsule.attach", &[]);
    span.in_scope(|| {
        jackin_diagnostics::operation_error("capsule.attach", error_type, body, &[]);
    });
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
    /// `Some(session_id)` when the client (typically the host
    /// console picking out of the snapshot preview) wants the daemon
    /// to focus a specific pane before forwarding content. The main
    /// loop calls `Multiplexer::focus_session_globally` on receipt.
    /// Unknown ids are silently ignored — see the daemon arm.
    pub(crate) focus_session: Option<u64>,
    pub(crate) client_permit: tokio::sync::OwnedSemaphorePermit,
}

pub(crate) struct ControlRequest {
    pub(crate) msg: jackin_protocol::control::ClientMsg,
    pub(crate) reply_tx: oneshot::Sender<jackin_protocol::control::ServerMsg>,
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
            crate::clog!("attach: handshake read_exact(first byte) failed: {e}");
            drop(client_permit);
            return;
        }
        Err(_) => {
            crate::clog!(
                "attach: handshake first byte not received within {HANDSHAKE_TIMEOUT:?}; dropping connection"
            );
            drop(client_permit);
            return;
        }
    }
    if first[0] == 0x00 {
        // Control channel — one-shot length-prefixed JSON. Decode off the
        // main loop, then ask the daemon loop to build the reply from current
        // state so refresh requests and usage APIs stay daemon-owned.
        let msg = match socket::read_control_msg(&mut stream, first[0]).await {
            Ok(msg) => msg,
            Err(e) => {
                crate::clog!("control: rejecting malformed request: {e:#}");
                drop(client_permit);
                return;
            }
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if control_tx.send(ControlRequest { msg, reply_tx }).is_err() {
            crate::clog!("control: daemon loop unavailable while handling request");
            drop(client_permit);
            return;
        }
        match tokio::time::timeout(HANDSHAKE_TIMEOUT, reply_rx).await {
            Ok(Ok(reply)) => socket::write_control_reply(stream, &reply).await,
            Ok(Err(_)) => crate::clog!("control: reply channel closed before response"),
            Err(_) => crate::clog!("control: daemon reply timed out after {HANDSHAKE_TIMEOUT:?}"),
        }
        drop(client_permit);
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
            crate::clog!("attach: handshake EOF before initial frame");
            drop(client_permit);
            return;
        }
        Ok(Err(e)) => {
            crate::clog!("attach: handshake frame decode failed: {e}");
            drop(client_permit);
            return;
        }
        Err(_) => {
            crate::clog!(
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
        focus_session,
    } = initial_frame
    else {
        crate::clog!("attach: rejected client whose first frame was not Hello: {initial_frame:?}");
        drop(client_permit);
        return;
    };
    let handshake = AttachHandshake {
        stream,
        rows,
        cols,
        spawn,
        env,
        terminal,
        focus_session,
        client_permit,
    };
    if handshake_tx.send(handshake).is_err() {
        crate::clog!("attach: handshake channel closed; daemon shutting down");
    }
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
        crate::clog!("{context}: client receiver already dropped; Shutdown frame not delivered");
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
                crate::clog!("{context}: attach task ended with join error: {err}");
            }
        }
        () = tokio::time::sleep(Duration::from_millis(ATTACH_SHUTDOWN_CLOSE_GRACE_MS)) => {
            crate::clog!(
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
pub(crate) async fn handle_attach_client(
    mut stream: UnixStream,
    mut out_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    cmd_tx: mpsc::UnboundedSender<ClientFrame>,
) {
    let mut tag = [0u8; 1];
    loop {
        tokio::select! {
            result = stream.read_exact(&mut tag) => {
                if let Err(e) = result {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        // Expected detach — not a failure (plan 008).
                        crate::cdebug!("attach client: socket closed (client detached)");
                    } else {
                        // Operator-visible breadcrumb + typed OTLP failure.
                        crate::cerror!("attach client: socket read failed: {e}");
                        record_attach_failure(
                            "attach_socket_read_failed",
                            "attach socket read failed",
                        );
                    }
                    break;
                }
                let frame = match read_client_frame(&mut stream, tag[0]).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        crate::cwarn!("attach client: EOF mid-frame (tag={:#04x})", tag[0]);
                        record_attach_failure(
                            "attach_socket_eof",
                            "attach socket closed mid-frame",
                        );
                        break;
                    }
                    Err(e) => {
                        crate::cerror!(
                            "attach client: frame decode failed (tag={:#04x}): {e}",
                            tag[0]
                        );
                        record_attach_failure(
                            "attach_frame_decode_failed",
                            "attach frame decode failed",
                        );
                        break;
                    }
                };
                if cmd_tx.send(frame).is_err() {
                    crate::clog!("attach client: cmd_tx closed; daemon shutting down");
                    return;
                }
            }
            Some(bytes) = out_rx.recv() => {
                if let Err(e) = stream.write_all(&bytes).await {
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::BrokenPipe
                    ) {
                        crate::cwarn!("attach client: socket write failed: {e}");
                    } else {
                        crate::cerror!("attach client: socket write failed: {e}");
                        record_attach_failure(
                            "attach_socket_write_failed",
                            "attach socket write failed",
                        );
                    }
                    break;
                }
            }
        }
    }
    // Signal the main loop that this client is gone so it can clear
    // `attached_out` / `attached_task` — without this, subsequent
    // `send_to_client` calls silently drop into the closed channel
    // and the daemon keeps treating the dead socket as live. If the
    // main loop is already shutting down the send fails; log so the
    // exact symptom this comment warns against does not happen
    // silently if the cmd_tx side is the one that died first.
    if cmd_tx.send(ClientFrame::Detach).is_err() {
        crate::clog!(
            "attach client: cmd_tx closed before synthetic Detach could fire; main loop is already tearing down"
        );
    }
}

#[cfg(test)]
mod tests {
    use tokio::{io::AsyncWriteExt, net::UnixStream, sync::mpsc};

    use super::{ClientFrame, handle_attach_client};

    #[tokio::test(flavor = "current_thread")]
    async fn conformance_attach_protocol_failure_and_expected_detach() {
        let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
        let guard = tracing::subscriber::set_default(subscriber);

        let (server, mut client) = UnixStream::pair().expect("create attach socket pair");
        let (_out_tx, out_rx) = mpsc::unbounded_channel();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        client.write_all(&[1]).await.expect("write frame tag");
        client.shutdown().await.expect("close frame mid-payload");
        handle_attach_client(server, out_rx, cmd_tx).await;
        assert!(matches!(cmd_rx.recv().await, Some(ClientFrame::Detach)));

        drop(guard);
        export.force_flush();
        assert_eq!(
            export.typed_error_count("capsule.attach", "attach_socket_eof"),
            1
        );
        assert_eq!(export.error_span_count(), 1);

        let (clean_export, clean_subscriber) =
            jackin_diagnostics::observability::test_capsule_layers(false);
        let clean_guard = tracing::subscriber::set_default(clean_subscriber);
        let (server, mut client) = UnixStream::pair().expect("create clean attach socket pair");
        let (_out_tx, out_rx) = mpsc::unbounded_channel();
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        client.shutdown().await.expect("close clean attach socket");
        handle_attach_client(server, out_rx, cmd_tx).await;
        assert!(matches!(cmd_rx.recv().await, Some(ClientFrame::Detach)));

        drop(clean_guard);
        clean_export.force_flush();
        assert_eq!(clean_export.error_span_count(), 0);
        assert_eq!(
            clean_export.typed_error_count("capsule.attach", "attach_socket_eof"),
            0
        );
    }
}
