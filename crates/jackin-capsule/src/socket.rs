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
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, mpsc};

use crate::protocol::control::{ClientMsg, ServerMsg, SessionInfo, frame};

/// Bail out of the accept loop after this many consecutive errors. The
/// dominant cause of repeating `accept` failures is fd exhaustion
/// (EMFILE) — busy-looping at 100% CPU and flooding the docker-logs
/// stream is worse than tearing down the listener so PID 1 can exit
/// and the container restarts cleanly.
const ACCEPT_FAILURE_BAIL: u32 = 10;

/// Hard cap on concurrent attach connections. The socket is locked
/// 0600 so only the agent uid can dial in, but a rogue in-container
/// process that shares the agent uid can otherwise open thousands
/// of sockets — each a tokio task + UnixStream fd. Reject excess
/// connections by closing immediately so the legitimate operator's
/// attach is never starved.
const MAX_CONCURRENT_CLIENTS: usize = 16;

/// Start the Unix socket listener. Returns a receiver of newly-connected
/// clients paired with the concurrency-cap permit. The caller (daemon)
/// must hold the permit alive for the lifetime of the spawned attach
/// task so the per-process cap correctly tracks live connections.
pub fn start_listener()
-> Result<mpsc::UnboundedReceiver<(UnixStream, tokio::sync::OwnedSemaphorePermit)>> {
    let path = Path::new(SOCKET_PATH);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        // Parent dir 0o700 so only the owner can list/connect. The
        // socket file itself gets 0o600 after bind, but on a system
        // where the parent dir is world-x an attacker can still
        // enumerate the path. Lock both. A chmod failure here is a
        // security regression: refuse to bind rather than continue
        // with a wider-than-intended attack surface. Operators on
        // exotic filesystems (NFS without owner perms) get an
        // actionable error instead of silent downgrade.
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("locking socket parent {} to 0o700", parent.display()))?;
    }

    let listener = UnixListener::bind(path)?;
    // Lock the socket to owner-only. Without this, any in-container
    // process that shares the agent uid (and any process running as a
    // different uid if the umask is generous) can connect and inject
    // `ClientFrame::Input` straight into the focused PTY. The attach
    // channel has no authentication beyond file-mode. Same hard-error
    // policy as the parent dir above.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("locking socket {SOCKET_PATH} to 0o600"))?;
    let (tx, rx) = mpsc::unbounded_channel();
    let limiter = Arc::new(Semaphore::new(MAX_CONCURRENT_CLIENTS));

    tokio::spawn(async move {
        let mut consecutive_failures = 0u32;
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    consecutive_failures = 0;
                    // try_acquire fails when MAX_CONCURRENT_CLIENTS
                    // tasks are already in flight. Drop the new
                    // socket without sending so a flood of in-uid
                    // peers cannot starve the legitimate operator's
                    // attach. Once a task finishes, its OwnedSemaphorePermit
                    // drops and a fresh accept proceeds.
                    let permit = match limiter.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => {
                            crate::clog!(
                                "socket: at concurrent-client cap {MAX_CONCURRENT_CLIENTS}; dropping new connection"
                            );
                            drop(stream);
                            continue;
                        }
                    };
                    if tx.send((stream, permit)).is_err() {
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
/// dispatcher). Returns `Err` with context on every failure mode so
/// callers can clog the underlying cause; the previous Option shape
/// collapsed read / oversize-len / decode errors into a silent None
/// and the host's `jackin status` then hung on its `read_exact`
/// waiting for a reply that was never coming.
pub async fn read_control_msg(stream: &mut UnixStream, first_byte: u8) -> Result<ClientMsg> {
    let mut rest = [0u8; 3];
    stream
        .read_exact(&mut rest)
        .await
        .context("control msg: reading length suffix")?;
    let len_buf = [first_byte, rest[0], rest[1], rest[2]];
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_CONTROL_MSG: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_MSG {
        anyhow::bail!("control msg length {len} exceeds limit {MAX_CONTROL_MSG}");
    }
    let mut body = vec![0u8; len];
    stream
        .read_exact(&mut body)
        .await
        .context("control msg: reading body")?;
    serde_json::from_slice(&body).context("control msg: parsing JSON body")
}

/// Handle a one-shot control request and close the connection.
pub async fn handle_control_request(
    mut stream: UnixStream,
    first_byte: u8,
    sessions: Vec<SessionInfo>,
    tabs: Vec<crate::protocol::control::TabSnapshot>,
    active_tab: u32,
) {
    let msg = match read_control_msg(&mut stream, first_byte).await {
        Ok(msg) => msg,
        Err(e) => {
            crate::clog!("control: rejecting malformed request: {e:#}");
            return;
        }
    };
    let reply = match msg {
        ClientMsg::Status => ServerMsg::SessionList { sessions },
        ClientMsg::Snapshot => ServerMsg::Snapshot { tabs, active_tab },
        ClientMsg::Unknown => {
            // Forward-compat: a peer running a newer host CLI sent a
            // variant this capsule does not understand yet. Reply with
            // ServerMsg::Unknown so the peer's `read_exact` returns
            // immediately rather than hanging until SOCKET_TIMEOUT.
            crate::clog!("control: ignoring unknown ClientMsg variant from peer");
            ServerMsg::Unknown
        }
    };
    if let Err(e) = stream.write_all(&frame(&reply)).await {
        crate::clog!("control reply write failed (msg={msg:?}): {e}");
    }
}
