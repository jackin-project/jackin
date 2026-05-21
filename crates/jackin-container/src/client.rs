/// Attach client — runs inside the container when `jackin-container` is
/// invoked with PID != 1. Sets the host terminal into raw mode, opens
/// the Unix socket, negotiates the binary attach channel, and shuttles
/// bytes between the operator's terminal and the multiplexer daemon.
use std::io::Write;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};

use crate::protocol::attach::{ClientFrame, ServerFrame, encode_client, read_server_frame};
use crate::protocol::control::{ClientMsg, ServerMsg, frame as control_frame};
use crate::socket::SOCKET_PATH;

/// Connect to the running daemon and run the interactive attach client.
pub async fn run_client(_new_session_agent: Option<String>) -> Result<()> {
    let (rows, cols) = terminal_size();

    crossterm::terminal::enable_raw_mode().context("failed to enable raw mode")?;
    let mut stdout = std::io::stdout();
    // Mouse: any-event tracking + SGR encoding.
    // Focus events: in / out.
    stdout.write_all(b"\x1b[?1003h\x1b[?1006h\x1b[?1004h")?;
    stdout.flush()?;

    let _cleanup = RawModeGuard;

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-container daemon — is it running?")?;

    stream
        .write_all(&encode_client(ClientFrame::Hello { rows, cols }))
        .await?;

    let mut stdin_buf = [0u8; 4096];
    let mut tag_buf = [0u8; 1];
    let mut tokio_stdin = tokio::io::stdin();
    let mut winch =
        signal(SignalKind::window_change()).context("failed to install SIGWINCH handler")?;

    loop {
        tokio::select! {
            // Read attach frame from daemon → stdout.
            result = stream.read_exact(&mut tag_buf) => {
                if result.is_err() { break; }
                let tag = tag_buf[0];
                let Ok(Some(frame)) = read_server_frame(&mut stream, tag).await else {
                    break;
                };
                match frame {
                    ServerFrame::Output(bytes) => {
                        let mut stdout = std::io::stdout();
                        let _ = stdout.write_all(&bytes);
                        let _ = stdout.flush();
                    }
                    ServerFrame::Shutdown => break,
                    ServerFrame::Bell => {
                        let _ = std::io::stdout().write_all(b"\x07");
                        let _ = std::io::stdout().flush();
                    }
                    ServerFrame::Welcome { .. } | ServerFrame::SessionList(_) => {}
                }
            }

            // Read stdin → daemon as Input frame.
            result = tokio_stdin.read(&mut stdin_buf) => {
                let n = match result {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let msg = encode_client(ClientFrame::Input(stdin_buf[..n].to_vec()));
                if stream.write_all(&msg).await.is_err() { break; }
            }

            // Outer terminal resize → propagate.
            _ = winch.recv() => {
                let (rows, cols) = terminal_size();
                let msg = encode_client(ClientFrame::Resize { rows, cols });
                if stream.write_all(&msg).await.is_err() { break; }
            }
        }
    }

    Ok(())
}

/// Query the daemon for current session list and print it.
pub async fn run_status() -> Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-container daemon")?;

    let msg = control_frame(&ClientMsg::Status);
    stream.write_all(&msg).await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let msg: ServerMsg = serde_json::from_slice(&body)?;
    let ServerMsg::SessionList { sessions } = msg;
    println!("Sessions: {}", sessions.len());
    for s in &sessions {
        println!(
            "  [{}] {} ({}) state={} active={}",
            s.id,
            s.label,
            s.agent.as_deref().unwrap_or("shell"),
            s.state.label(),
            s.active,
        );
    }

    Ok(())
}

/// Return the outer terminal size as `(rows, cols)`.
///
/// `crossterm::terminal::size()` returns `(columns, rows)`. Failing to
/// flip the pair lands the agent's PTY with `rows` and `cols` swapped:
/// a 50-row × 200-col terminal becomes a 200-row × 50-col PTY, the
/// status bar renders at 50 cols, and agent output wraps far too
/// short. The fix is one line — keep the flip explicit so a future
/// reader sees the convention difference at the call site.
fn terminal_size() -> (u16, u16) {
    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    (rows, cols)
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        // Disable mouse, focus events, restore cursor.
        let _ = std::io::stdout().write_all(b"\x1b[?1003l\x1b[?1006l\x1b[?1004l\x1b[?25h");
        let _ = std::io::stdout().flush();
    }
}
