// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule TUI event loop: drives the ratatui render loop, processes input
//! bytes from the attach protocol, and dispatches effects to the daemon.
//!
//! Not responsible for: daemon-side session management or PTY I/O — those
//! live in `daemon` and `pty`; this module is the client-side attach loop.
//!
//! Key invariant: `run_client` owns the raw terminal for its entire lifetime;
//! `enter_attach_terminal` installs cleanup via drop guard so the host
//! terminal is always restored even on panic or early return.

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

/// Connect to the running daemon and run the interactive attach client.
///
/// `spawn_request` is set by `docker exec ... jackin-capsule new`;
/// the first Hello frame asks the daemon to create that session before
/// completing attach. Plain attach (operator-initiated reattach)
/// passes `None`.
pub async fn run_client(
    spawn_request: Option<SpawnRequest>,
    focus_session: Option<u64>,
) -> Result<()> {
    let (rows, cols) = terminal_size();

    let mut stdout = std::io::stdout();
    let _cleanup = enter_attach_terminal(&mut stdout)?;

    let mut tokio_stdin = tokio::io::stdin();
    let mut terminal = ClientTerminal::from_env();
    // Query before connecting: once the Hello lands, every stdin byte
    // forwards to the daemon as pane input, so a reply arriving later would
    // land in the focused agent's PTY as keystrokes.
    let host_colors = crate::tui::host_colors::query_host_terminal_colors(
        terminal.term.as_deref(),
        &mut tokio_stdin,
        &mut stdout,
    )
    .await;
    terminal.default_fg = host_colors.fg;
    terminal.default_bg = host_colors.bg;

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon — is it running?")?;

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
        // Keystrokes typed during the color query window.
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
                    ServerFrame::HostOpenUrl(url) => {
                        let redacted = crate::tui::url_text::redact_url_for_log(&url);
                        jackin_diagnostics::telemetry_debug!("capsule",
                            "attach-client: ignoring host-open-url frame in in-container client: {redacted:?}"
                        );
                    }
                    ServerFrame::HostRevealPath(_) => {
                        jackin_diagnostics::telemetry_debug!("capsule",
                            "attach-client: ignoring host-reveal-path frame in in-container client"
                        );
                    }
                    ServerFrame::HostStageImageFromClipboardPath => {
                        jackin_diagnostics::telemetry_debug!("capsule",
                            "attach-client: ignoring host-stage-image-path frame in in-container client"
                        );
                    }
                    ServerFrame::HostPasteImageFromClipboard => {
                        jackin_diagnostics::telemetry_debug!("capsule",
                            "attach-client: ignoring host-paste-image frame in in-container client"
                        );
                    }
                    ServerFrame::HostStageImageFromClipboard => {
                        jackin_diagnostics::telemetry_debug!("capsule",
                            "attach-client: ignoring host-stage-image frame in in-container client"
                        );
                    }
                    ServerFrame::FileExportStart(_)
                    | ServerFrame::FileExportChunk(_)
                    | ServerFrame::FileExportEnd(_) => {
                        jackin_diagnostics::telemetry_debug!("capsule","attach-client: ignoring host file-export frame");
                    }
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
