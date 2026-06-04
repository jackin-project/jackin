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
    ClientFrame, ClientTerminal, ServerFrame, SpawnRequest, encode_client, read_server_frame,
};
use crate::protocol::control::{ClientMsg, ServerMsg, frame as control_frame};
use crate::session::{SESSION_ENV_PASSTHROUGH, Session};
use crate::socket::SOCKET_PATH;
use crate::terminal_geometry::{DEFAULT_COLS, DEFAULT_ROWS, normalize_size};

/// Terminal-reset escape bytes written when the attach client detaches, minus
/// the alternate-screen leave (`?1049l`). The leave is appended only when this
/// client entered its own alternate screen — see [`outer_terminal_reset_sequence`].
const OUTER_TERMINAL_RESET_BASE: &[u8] =
    b"\x1b]22;default\x1b\\\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1007l\x1b[?1004l\x1b[?2004l\x1b[?1l\x1b[<u\x1b[?25h";
const ALTERNATE_SCREEN_LEAVE: &[u8] = b"\x1b[?1049l";

/// True when the host orchestrator owns one continuous alternate screen for the
/// whole launch flow and asked this attach client (via `JACKIN_HOST_ALT_SCREEN`
/// on the `docker exec`) not to toggle its own. Skipping the toggle keeps the
/// flow on a single screen so detaching the capsule does not pop the operator
/// back to the cooked terminal mid-flow.
fn host_owns_alt_screen() -> bool {
    std::env::var_os("JACKIN_HOST_ALT_SCREEN").is_some()
}

fn outer_terminal_reset_sequence() -> Vec<u8> {
    let mut seq = OUTER_TERMINAL_RESET_BASE.to_vec();
    if !host_owns_alt_screen() {
        seq.extend_from_slice(ALTERNATE_SCREEN_LEAVE);
    }
    seq
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
    if host_owns_alt_screen() {
        // Host already holds the alternate screen; clear + home to wipe the
        // loading frame and draw fresh, but don't enter (and later leave) our
        // own buffer.
        stdout.write_all(b"\x1b[2J\x1b[H")?;
    } else {
        stdout.write_all(b"\x1b[?1049h\x1b[2J\x1b[H")?;
    }
    stdout.write_all(Session::client_owned_mode_state())?;
    stdout.flush()?;

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
                let msg = match encode_client(ClientFrame::Input(stdin_buf[..n].to_vec())) {
                    Ok(bytes) => bytes,
                    Err(e) => break Err(e.context("encoding Input frame")),
                };
                if let Err(e) = stream.write_all(&msg).await {
                    break Err(anyhow::anyhow!("attach socket write failed (input): {e}"));
                }
            }

            // Outer terminal resize → propagate.
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
        ServerMsg::AgentRegistry { .. } => {
            anyhow::bail!("daemon replied with AgentRegistry for Status request")
        }
        _ => anyhow::bail!("daemon replied with unexpected message type for Status request"),
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
        ServerMsg::AgentRegistry { .. } => {
            anyhow::bail!("daemon replied with AgentRegistry for Snapshot request")
        }
        _ => anyhow::bail!("daemon replied with unexpected message type for Snapshot request"),
    };
    let payload = serde_json::json!({
        "tabs": tabs,
        "active_tab": active_tab,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

/// Format for `jackin-capsule agents` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentsFormat {
    Human,
    Json,
}

/// Query the daemon for the agent registry and render it.
///
/// `--format json` emits the registry as a JSON array.
/// Human format renders a table with a `← you` annotation on the caller's row.
pub async fn run_agents(format: AgentsFormat) -> Result<()> {
    let mut stream = UnixStream::connect(SOCKET_PATH)
        .await
        .context("cannot connect to jackin-capsule daemon")?;

    stream.write_all(&control_frame(&ClientMsg::Agents)).await?;

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
    let records = match msg {
        ServerMsg::AgentRegistry { records } => records,
        ServerMsg::Unknown => {
            anyhow::bail!(
                "daemon replied with ServerMsg::Unknown for Agents — peer is newer than this CLI"
            )
        }
        ServerMsg::SessionList { .. } => {
            anyhow::bail!("daemon replied with SessionList for Agents request")
        }
        ServerMsg::Snapshot { .. } => {
            anyhow::bail!("daemon replied with Snapshot for Agents request")
        }
        _ => anyhow::bail!("daemon replied with unexpected message type for Agents request"),
    };

    // Determine caller's own codename and annotate matching records.
    let my_codename = std::env::var("JACKIN_AGENT_CODENAME").unwrap_or_default();
    let mut records = records;
    if !my_codename.is_empty() {
        for r in &mut records {
            r.is_self = r.codename == my_codename;
        }
    }

    if format == AgentsFormat::Json {
        println!("{}", serde_json::to_string_pretty(&records)?);
        return Ok(());
    }

    print!("{}", jackin_tui::ansi::BRAND_BANNER);
    println!("agent registry");
    if let Some(r) = records.iter().find(|r| r.is_self) {
        println!(
            "\nYou are: {} ({} · {})",
            r.codename,
            r.agent.as_deref().unwrap_or("shell"),
            r.provider.as_deref().unwrap_or("—"),
        );
    }

    // Split active first, then exited — within each group sort by started_at.
    let mut active: Vec<_> = records.iter().filter(|r| r.status == "active").collect();
    let mut exited: Vec<_> = records.iter().filter(|r| r.status != "active").collect();
    active.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    exited.sort_by(|a, b| a.started_at.cmp(&b.started_at));

    println!();
    println!(
        "  {:<12} {:<10} {:<14} {:<20} {:<20} status",
        "codename", "agent", "provider", "started", "exited"
    );
    println!("  {}", "─".repeat(83));

    for r in active.iter().chain(exited.iter()) {
        let you = if r.is_self { "  ← you" } else { "" };
        println!(
            "  {:<12} {:<10} {:<14} {:<20} {:<20} {}{}",
            r.codename,
            r.agent.as_deref().unwrap_or("shell"),
            r.provider.as_deref().unwrap_or("—"),
            // Trim the trailing 'Z' and 'T' for compact display
            r.started_at.trim_end_matches('Z').replace('T', " "),
            r.exited_at
                .as_deref()
                .unwrap_or("—")
                .trim_end_matches('Z')
                .replace('T', " "),
            r.status,
            you,
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
        // Failures here leave the operator's host terminal in raw mode
        // + alt-screen + mouse tracking on, which they would only
        // discover when keystrokes stop echoing. Surface each failure
        // on stderr so they have a fighting chance to `reset` manually.
        //
        // Write the outer-terminal reset BEFORE disabling raw mode:
        // if the write fails but disable succeeds, the operator at
        // least gets cooked mode; if disable fails but the reset
        // already shipped, the visible state matches the escape codes.
        let mut stdout = std::io::stdout().lock();
        let write_result = stdout
            .write_all(&outer_terminal_reset_sequence())
            .and_then(|_| stdout.flush());
        drop(stdout);
        let log = |label: &str, e: &dyn std::fmt::Display| {
            eprintln!("[jackin-capsule] failed to {label} on detach: {e}");
        };
        if let Err(e) = write_result {
            log("write outer-terminal reset", &e);
        }
        if let Err(e) = crossterm::terminal::disable_raw_mode() {
            log("disable raw mode", &e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outer_terminal_reset_disables_alternate_scroll() {
        let reset = outer_terminal_reset_sequence();
        let needle = b"\x1b[?1007l";
        assert!(
            reset.windows(needle.len()).any(|w| w == needle),
            "outer terminal reset missing alternate-scroll disable: {reset:?}"
        );
    }

    #[test]
    fn reset_base_excludes_alt_screen_leave() {
        // The base never carries the alternate-screen leave; it is appended
        // only when this client owns its own screen. A host-owned flow keeps
        // the leave out so detaching does not pop to the cooked terminal.
        assert!(
            !OUTER_TERMINAL_RESET_BASE
                .windows(ALTERNATE_SCREEN_LEAVE.len())
                .any(|w| w == ALTERNATE_SCREEN_LEAVE),
            "reset base must not contain the alternate-screen leave"
        );
        let mut full = OUTER_TERMINAL_RESET_BASE.to_vec();
        full.extend_from_slice(ALTERNATE_SCREEN_LEAVE);
        assert!(full.ends_with(ALTERNATE_SCREEN_LEAVE));
    }
}
