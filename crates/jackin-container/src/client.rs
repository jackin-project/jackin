/// Interactive client mode — runs when PID != 1.
///
/// Connects to the daemon socket, forwards stdin → daemon, writes
/// daemon output → stdout. Also handles local rendering for the
/// Ctrl+J command palette (sends commands to daemon via socket).
use std::io::Write;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::protocol::{ClientMsg, ServerMsg, b64_decode, b64_encode, frame};
use crate::socket::SOCKET_PATH;

/// Connect to the running daemon and run the interactive client loop.
pub async fn run_client(new_session_agent: Option<String>) -> Result<()> {
    let (rows, cols) = terminal_size();

    // Enable raw mode on stdin.
    crossterm::terminal::enable_raw_mode()
        .context("failed to enable raw mode")?;
    // Enable mouse reporting.
    let mut stdout = std::io::stdout();
    stdout.write_all(b"\x1b[?1003h\x1b[?1006h")?; // SGR + any-event mouse
    stdout.flush()?;

    let _cleanup = RawModeGuard;

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-container daemon — is it running?")?;

    // Handshake.
    let hello = frame(&ClientMsg::Hello { rows, cols });
    stream.write_all(&hello).await?;

    // If caller wants a new session, send that after welcome.
    let mut pending_new_session = new_session_agent;

    // Split stream for concurrent read/write.
    // We use a manual loop with select! over stdin + socket.
    let mut stdin_buf = [0u8; 4096];
    let mut sock_len_buf = [0u8; 4];
    let mut tokio_stdin = tokio::io::stdin();

    loop {
        tokio::select! {
            // Read from socket (daemon → terminal).
            result = stream.read_exact(&mut sock_len_buf) => {
                if result.is_err() { break; }
                let len = u32::from_be_bytes(sock_len_buf) as usize;
                if len > 4 * 1024 * 1024 { break; }
                let mut body = vec![0u8; len];
                if stream.read_exact(&mut body).await.is_err() { break; }
                let Ok(msg) = serde_json::from_slice::<ServerMsg>(&body) else { continue };
                match msg {
                    ServerMsg::Output { data } => {
                        let bytes = b64_decode(&data);
                        let mut stdout = std::io::stdout();
                        let _ = stdout.write_all(&bytes);
                        let _ = stdout.flush();
                    }
                    ServerMsg::Shutdown => {
                        // Daemon is done — restore terminal and exit.
                        break;
                    }
                    ServerMsg::SessionList { .. } => {}
                    _ => {}
                }

                // After welcome, send pending new-session request.
                if let Some(agent) = pending_new_session.take() {
                    let msg = if agent.is_empty() {
                        frame(&ClientMsg::NewSession { agent: None })
                    } else {
                        frame(&ClientMsg::NewSession { agent: Some(agent) })
                    };
                    let _ = stream.write_all(&msg).await;
                }
            }

            // Read from stdin (terminal → daemon).
            result = tokio_stdin.read(&mut stdin_buf) => {
                let n = match result {
                    Ok(0) | Err(_) => break,
                    Ok(n) => n,
                };
                let data = b64_encode(&stdin_buf[..n]);
                let msg = frame(&ClientMsg::Input { data });
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

    let msg = frame(&ClientMsg::Status);
    stream.write_all(&msg).await?;

    // Read response.
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let msg: ServerMsg = serde_json::from_slice(&body)?;
    if let ServerMsg::SessionList { sessions } = msg {
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
    }

    Ok(())
}

fn terminal_size() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((24, 80))
}

/// RAII guard to restore the terminal on drop.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        // Disable mouse reporting.
        let _ = std::io::stdout().write_all(b"\x1b[?1003l\x1b[?1006l");
        // Show cursor.
        let _ = std::io::stdout().write_all(b"\x1b[?25h");
        let _ = std::io::stdout().flush();
    }
}
