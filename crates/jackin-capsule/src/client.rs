/// Attach client — runs inside the container when `jackin-capsule` is
/// invoked with PID != 1. Sets the host terminal into raw mode, opens
/// the Unix socket, negotiates the binary attach channel, and shuttles
/// bytes between the operator's terminal and the multiplexer daemon.
use std::io::Write;

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::signal::unix::{SignalKind, signal};

use crate::protocol::attach::{
    ClientFrame, ServerFrame, SpawnRequest, encode_client, read_server_frame,
};
use crate::protocol::control::{ClientMsg, ServerMsg, frame as control_frame};
use crate::session::{SESSION_ENV_PASSTHROUGH, Session};
use crate::socket::SOCKET_PATH;
use crate::terminal_geometry::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};

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

    crossterm::terminal::enable_raw_mode().context("failed to enable raw mode")?;
    // Install the cleanup guard BEFORE writing the alt-screen ENTER
    // sequence: if the write returns Err, Drop on the guard still
    // exits raw mode + the alt-screen buffer so the operator's host
    // terminal stays usable.
    let _cleanup = RawModeGuard;
    let mut stdout = std::io::stdout();
    // Enter the alternate-screen buffer so the multiplexer's draw
    // calls do not append to the outer terminal's scrollback. Without
    // this:
    //  - the operator can scroll the host terminal past the live
    //    daemon output and see stale frame history pile up;
    //  - text selection in the host terminal spans those stale rows
    //    AND the live frame, picking up content far outside the
    //    intended pane;
    //  - resize re-draws stack on top of old ones because the host
    //    keeps the old content above the cursor.
    // Mouse: any-event tracking + SGR encoding. Focus events on.
    stdout.write_all(b"\x1b[?1049h\x1b[2J\x1b[H")?;
    stdout.write_all(Session::client_owned_mode_state())?;
    stdout.flush()?;

    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon — is it running?")?;

    stream
        .write_all(&encode_client(ClientFrame::Hello {
            rows,
            cols,
            env: collect_session_env(spawn_request.is_some()),
            spawn: spawn_request,
            focus_session,
        }))
        .await?;

    let mut stdin_buf = [0u8; 4096];
    let mut tag_buf = [0u8; 1];
    let mut tokio_stdin = tokio::io::stdin();
    let mut winch =
        signal(SignalKind::window_change()).context("failed to install SIGWINCH handler")?;

    // Track why the loop broke. `Some(())` = clean detach (received
    // Shutdown / clean stdin EOF); `None` initially means "still in
    // the loop." `Err` paths set a contextual `anyhow::Error` so the
    // CLI returns non-zero — operator can tell clean detach from a
    // daemon crash / broken pipe.
    let exit_result: Result<()> = loop {
        tokio::select! {
            // Read attach frame from daemon → stdout.
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

            // Read stdin → daemon as Input frame.
            result = tokio_stdin.read(&mut stdin_buf) => {
                let n = match result {
                    Ok(0) => break Ok(()),
                    Err(e) => break Err(anyhow::anyhow!("stdin read failed: {e}")),
                    Ok(n) => n,
                };
                let msg = encode_client(ClientFrame::Input(stdin_buf[..n].to_vec()));
                if let Err(e) = stream.write_all(&msg).await {
                    break Err(anyhow::anyhow!("attach socket write failed (input): {e}"));
                }
            }

            // Outer terminal resize → propagate.
            _ = winch.recv() => {
                let (rows, cols) = terminal_size();
                let msg = encode_client(ClientFrame::Resize { rows, cols });
                if let Err(e) = stream.write_all(&msg).await {
                    break Err(anyhow::anyhow!("attach socket write failed (resize): {e}"));
                }
            }
        }
    };

    exit_result
}

/// Query the daemon for current session list and print it.
pub async fn run_status() -> Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;

    let msg = control_frame(&ClientMsg::Status);
    stream.write_all(&msg).await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    // Mirror the daemon-side cap in `socket::read_control_msg`. A
    // buggy or wedged daemon (or a peer that won the socket race
    // inside the container) could otherwise send `0xFFFFFFFF` and
    // force a 4 GiB allocation attempt in the client.
    const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_REPLY {
        anyhow::bail!("daemon control reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let msg: ServerMsg = serde_json::from_slice(&body)?;
    let sessions = match msg {
        ServerMsg::SessionList { sessions } => sessions,
        ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Status — peer is newer than this CLI"
            )
        }
        ServerMsg::Snapshot { .. } => {
            anyhow::bail!("daemon replied with Snapshot for Status request")
        }
    };
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

/// Query the daemon for the tab/pane snapshot and print as JSON.
/// Output shape is `ServerMsg::Snapshot` verbatim so the host
/// console can deserialize the same struct it shares with the
/// daemon — no second schema to keep in sync.
pub async fn run_snapshot() -> Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;

    stream
        .write_all(&control_frame(&ClientMsg::Snapshot))
        .await?;

    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_REPLY {
        anyhow::bail!("daemon control reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let msg: ServerMsg = serde_json::from_slice(&body)?;
    let (tabs, active_tab) = match msg {
        ServerMsg::Snapshot { tabs, active_tab } => (tabs, active_tab),
        ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Snapshot — peer is newer than this CLI"
            )
        }
        ServerMsg::SessionList { .. } => {
            anyhow::bail!("daemon replied with SessionList for Snapshot request")
        }
    };
    let payload = serde_json::json!({
        "tabs": tabs,
        "active_tab": active_tab,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
    );
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
    let (cols, rows) = crossterm::terminal::size().unwrap_or((DEFAULT_COLS, DEFAULT_ROWS));
    normalize_size(rows, cols)
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

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        // Reset every outer-terminal mode the client or focused pane
        // may have enabled before returning the operator to their
        // host terminal.
        let _ = std::io::stdout().write_all(
            b"\x1b]22;default\x1b\\\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b[<u\x1b[?25h\x1b[?1049l",
        );
        let _ = std::io::stdout().flush();
    }
}
