use std::io::Write;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};

use crate::protocol::attach::{
    ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_client, read_server_frame,
};
use crate::session::SESSION_ENV_PASSTHROUGH;
use crate::socket::SOCKET_PATH;
use crate::tui::terminal::{enter_attach_terminal, terminal_size};

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

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon — is it running?")?;

    let hello = encode_client(ClientFrame::Hello {
        rows,
        cols,
        env: collect_session_env(spawn_request.is_some()),
        spawn: spawn_request,
        terminal: ClientTerminal::from_env(),
        focus_session,
    })
    .context("encoding attach Hello frame")?;
    stream.write_all(&hello).await?;

    let mut stdin_buf = [0u8; 4096];
    let mut tag_buf = [0u8; 1];
    let mut tokio_stdin = tokio::io::stdin();
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
                    ServerFrame::Shutdown => break Ok(()),
                    ServerFrame::Bell => {
                        let mut stdout = std::io::stdout();
                        if let Err(e) = stdout.write_all(b"\x07") {
                            break Err(anyhow::anyhow!("stdout closed while writing Bell: {e}"));
                        }
                        if let Err(e) = stdout.flush() {
                            break Err(anyhow::anyhow!("stdout flush failed after Bell: {e}"));
                        }
                    }
                    ServerFrame::Welcome { .. } | ServerFrame::SessionList(_) => {}
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

fn collect_session_env(include: bool) -> Vec<(String, String)> {
    if !include {
        return Vec::new();
    }
    SESSION_ENV_PASSTHROUGH
        .iter()
        .filter_map(|&key| {
            std::env::var(key)
                .ok()
                .map(|value| (key.to_string(), value))
        })
        .collect()
}
