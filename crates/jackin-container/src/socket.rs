/// Unix domain socket server.
///
/// Listens on `/run/jackin/jackin.sock`. Two protocols share the socket:
/// the **control channel** (length-prefixed JSON, used by the host CLI
/// for one-shot queries) and the **attach channel** (binary tag+length
/// frames, used by interactive clients). The two are disambiguated by
/// the first byte of the connection — `0x00` means a length prefix
/// (control), anything else is an attach-channel tag.
pub const SOCKET_PATH: &str = "/run/jackin/jackin.sock";

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use crate::protocol::control::{ClientMsg, ServerMsg, SessionInfo, frame};

/// Bail out of the accept loop after this many consecutive errors. The
/// dominant cause of repeating `accept` failures is fd exhaustion
/// (EMFILE) — busy-looping at 100% CPU and flooding the docker-logs
/// stream is worse than tearing down the listener so PID 1 can exit
/// and the container restarts cleanly.
const ACCEPT_FAILURE_BAIL: u32 = 10;

/// Start the Unix socket listener. Returns a receiver of newly-connected
/// clients. The caller (daemon) accepts clients from the channel.
pub fn start_listener() -> Result<mpsc::UnboundedReceiver<UnixStream>> {
    let path = Path::new(SOCKET_PATH);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        // Parent dir 0o700 so only the owner can list/connect. The
        // socket file itself gets 0o600 after bind, but on a system
        // where the parent dir is world-x an attacker can still
        // enumerate the path. Lock both.
        if let Err(e) = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)) {
            crate::clog!("socket: failed to chmod parent {}: {e}", parent.display());
        }
    }

    let listener = UnixListener::bind(path)?;
    // Lock the socket to owner-only. Without this, any in-container
    // process that shares the agent uid (and any process running as a
    // different uid if the umask is generous) can connect and inject
    // `ClientFrame::Input` straight into the focused PTY. The attach
    // channel has no authentication beyond file-mode.
    if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
        crate::clog!("socket: failed to chmod {SOCKET_PATH}: {e}");
    }
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut consecutive_failures = 0u32;
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    consecutive_failures = 0;
                    if tx.send(stream).is_err() {
                        // Receiver dropped — daemon is shutting down.
                        // Stop accepting so we don't burn cycles
                        // accepting connections that are immediately
                        // dropped on the floor.
                        crate::clog!("socket: client queue closed; listener stopping");
                        return;
                    }
                }
                Err(e) => {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    crate::clog!(
                        "socket accept error ({consecutive_failures}/{ACCEPT_FAILURE_BAIL}): {e}"
                    );
                    if consecutive_failures >= ACCEPT_FAILURE_BAIL {
                        crate::clog!(
                            "socket: giving up after {ACCEPT_FAILURE_BAIL} consecutive accept failures"
                        );
                        return;
                    }
                    // Exponential backoff capped at 5s so an EMFILE
                    // storm doesn't spin the runtime.
                    let backoff_ms = 50u64
                        .saturating_mul(1u64 << consecutive_failures.min(7))
                        .min(5_000);
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    });

    Ok(rx)
}

/// Read a length-prefixed JSON control message whose 4-byte length
/// prefix begins with `first_byte = 0x00` (already consumed by the
/// dispatcher).
pub async fn read_control_msg(stream: &mut UnixStream, first_byte: u8) -> Option<ClientMsg> {
    let mut rest = [0u8; 3];
    stream.read_exact(&mut rest).await.ok()?;
    let len_buf = [first_byte, rest[0], rest[1], rest[2]];
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 4 * 1024 * 1024 {
        return None;
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await.ok()?;
    serde_json::from_slice(&body).ok()
}

/// Handle a one-shot control request and close the connection.
pub async fn handle_control_request(
    mut stream: UnixStream,
    first_byte: u8,
    sessions: Vec<SessionInfo>,
) {
    let Some(msg) = read_control_msg(&mut stream, first_byte).await else {
        return;
    };
    let reply = match msg {
        ClientMsg::Status => ServerMsg::SessionList { sessions },
    };
    if let Err(e) = stream.write_all(&frame(&reply)).await {
        crate::clog!("control reply write failed (msg={msg:?}): {e}");
    }
}
