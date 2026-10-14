// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};

use crate::protocol::attach::{
    ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_client, read_server_frame,
};
use crate::socket::SOCKET_PATH;
use crate::tui::terminal::{enter_attach_terminal, terminal_size};

fn telemetry_context() -> Option<Box<jackin_protocol::TelemetryContext>> {
    let mut context = jackin_protocol::TelemetryContext::v1();
    jackin_telemetry::propagation::inject(&mut context);
    Some(Box::new(context))
}

pub async fn run_client(
    spawn_request: Option<SpawnRequest>,
    focus_session: Option<u64>,
) -> Result<()> {
    let (rows, cols) = terminal_size();

    let mut stdout = std::io::stdout();
    let _cleanup = enter_attach_terminal(&mut stdout)?;

    let mut tokio_stdin = tokio::io::stdin();
    let mut terminal = ClientTerminal::from_env();

    let host_colors = jackin_protocol::host_terminal::query_host_terminal_colors(
        terminal.term.as_deref(),
        &mut tokio_stdin,
        &mut stdout,
    )
    .await;
    terminal.default_fg = host_colors.fg;
    terminal.default_bg = host_colors.bg;

    let mut stream = connect_attach_socket().await?;

    let hello = encode_client(ClientFrame::Hello {
        rows,
        cols,
        env: crate::attach_context::collect_session_env(spawn_request.is_some()),
        spawn: spawn_request,
        terminal,
        focus_session,
        context: telemetry_context(),
    })
    .context("encoding attach Hello frame")?;
    stream
        .write_all(&hello)
        .await
        .context("sending attach Hello frame")?;
    if !host_colors.leftover_input.is_empty() {
        let msg = encode_client(ClientFrame::Input(host_colors.leftover_input))
            .context("encoding pre-attach Input frame")?;
        stream
            .write_all(&msg)
            .await
            .context("sending pre-attach Input frame")?;
    }

    let mut stdin_buf = [0u8; 4096];
    let mut tag_buf = [0u8; 1];
    let mut winch =
        signal(SignalKind::window_change()).context("failed to install SIGWINCH handler")?;

    loop {
        tokio::select! {
            result = stream.read_exact(&mut tag_buf) => {
                if let Err(e) = result {
                    break Err(anyhow::anyhow!("attach socket closed unexpectedly: {e}"));
                }
                let tag = tag_buf[0];
                let frame = match read_server_frame(&mut stream, tag).await {
                    Ok(Some(frame)) => frame,
                    Ok(None) => break Err(anyhow::anyhow!(
                        "attach socket EOF mid-frame (tag={tag:#04x})"
                    )),
                    Err(e) => break Err(anyhow::anyhow!(
                        "decoding server frame (tag={tag:#04x}): {e}"
                    )),
                };
                match frame {
                    ServerFrame::Output(bytes) => {
                        let mut stdout = std::io::stdout();
                        if let Err(e) = stdout.write_all(&bytes) {
                            break Err(anyhow::anyhow!(
                                "stdout closed while writing Output ({} bytes): {e}",
                                bytes.len()
                            ));
                        }
                        if let Err(e) = stdout.flush() {
                            break Err(anyhow::anyhow!("stdout flush failed: {e}"));
                        }
                    }
                    ServerFrame::Shutdown { reason } => {
                        if let Some(reason) = reason {
                            break Err(anyhow::anyhow!(reason));
                        }
                        break Ok(());
                    }
                    ServerFrame::Bell => {
                        let mut stdout = std::io::stdout();
                        if let Err(e) = stdout.write_all(b"\x07") {
                            break Err(anyhow::anyhow!("stdout closed while writing Bell: {e}"));
                        }
                        if let Err(e) = stdout.flush() {
                            break Err(anyhow::anyhow!("stdout flush failed after Bell: {e}"));
                        }
                    }
                    ServerFrame::HostOpenUrl(_)
                    | ServerFrame::HostRevealPath(_)
                    | ServerFrame::HostStageImageFromClipboardPath
                    | ServerFrame::HostPasteImageFromClipboard
                    | ServerFrame::HostStageImageFromClipboard
                    | ServerFrame::FileExportStart(_)
                    | ServerFrame::FileExportChunk(_)
                    | ServerFrame::FileExportEnd(_) => {}
                    ServerFrame::Welcome { .. }
                    | ServerFrame::SessionList(_)
                    | ServerFrame::AttachControlResponse(_) => {}
                }
            }

            result = tokio_stdin.read(&mut stdin_buf) => {
                let n = match result {
                    Ok(0) => break Ok(()),
                    Err(e) => break Err(anyhow::anyhow!("stdin read failed: {e}")),
                    Ok(n) => n,
                };
                let msg = match encode_client(ClientFrame::Input(stdin_buf[..n].to_vec())) {
                    Ok(bytes) => bytes,
                    Err(e) => break Err(e.context("encoding Input frame")),
                };
                if let Err(e) = stream.write_all(&msg).await {
                    break Err(anyhow::anyhow!("attach socket write failed (input): {e}"));
                }
            }

            _ = winch.recv() => {
                let (rows, cols) = terminal_size();
                let msg = match encode_client(ClientFrame::Resize { rows, cols }) {
                    Ok(bytes) => bytes,
                    Err(e) => break Err(e.context("encoding Resize frame")),
                };
                if let Err(e) = stream.write_all(&msg).await {
                    break Err(anyhow::anyhow!("attach socket write failed (resize): {e}"));
                }
            }
        }
    }
}

async fn connect_attach_socket() -> Result<UnixStream> {
    let mut stream = jackin_diagnostics::operation::connection_attempt(
        jackin_telemetry::schema::enums::ConnectionPeerType::CapsuleAttach,
        UnixStream::connect(SOCKET_PATH),
    )
    .await
    .context("cannot connect to jackin-capsule daemon — is it running?")?;
    jackin_protocol::capsule_transport::client_handshake_async(&mut stream)
        .await
        .context("negotiating Capsule attach transport")?;
    Ok(stream)
}
