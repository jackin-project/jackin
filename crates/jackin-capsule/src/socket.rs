/// Unix domain socket server.
///
/// Listens on `/jackin/run/jackin.sock`. Two protocols share the socket:
/// the **control channel** (length-prefixed JSON, used by the host CLI
/// for one-shot queries) and the **attach channel** (binary tag+length
/// frames, used by interactive clients). The two are disambiguated by
/// the first byte of the connection — `0x00` means a length prefix
/// (control), anything else is an attach-channel tag.
///
/// Path lives under the `/jackin/` container-root convention (see
/// AGENTS.md "Container path convention"): every jackin-owned mount,
/// runtime asset, and runtime state directory sits beneath `/jackin/`
/// so an operator can `ls /jackin/` to find all jackin-controlled
/// state in one place.
pub const SOCKET_PATH: &str = "/jackin/run/jackin.sock";

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
    start_listener_at(Path::new(SOCKET_PATH))
}

/// Bind a `UnixListener` at `path` with the parent dir locked to 0o700
/// and the socket file to 0o600. Pulled out for tests; production
/// callers go through `start_listener`.
pub(crate) fn start_listener_at(
    path: &Path,
) -> Result<mpsc::UnboundedReceiver<(UnixStream, tokio::sync::OwnedSemaphorePermit)>> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("removing stale socket {}", path.display()));
        }
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
        .with_context(|| format!("locking socket {} to 0o600", path.display()))?;
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
                    // Exponential backoff capped at 5 s so an EMFILE
                    // storm doesn't spin the runtime. The 5 s `.min()`
                    // is the load-bearing cap; the shift cap at 16 is
                    // dead-code defence against a future operator
                    // raising `ACCEPT_FAILURE_BAIL` past the current
                    // ladder and tripping the `1u64 << N` UB shift.
                    let shift = consecutive_failures.saturating_sub(1).min(16);
                    let backoff_ms = 50u64.saturating_mul(1u64 << shift).min(5_000);
                    crate::clog!("socket: backing off {backoff_ms}ms before next accept");
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
            // Reply with `Unknown` so the peer's `read_exact` returns
            // immediately rather than hanging until SOCKET_TIMEOUT.
            crate::clog!("control: ignoring unknown ClientMsg variant from peer");
            ServerMsg::Unknown
        }
    };
    // Bound the reply write so a peer that disappeared between request
    // decode and reply write cannot wedge this task forever holding the
    // attach-concurrency permit. 2 s is generous for a single localhost
    // socket write; anything slower is the peer being unresponsive.
    match tokio::time::timeout(Duration::from_secs(2), stream.write_all(&frame(&reply))).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => crate::clog!("control reply write failed (msg={msg:?}): {e}"),
        Err(_) => crate::clog!("control reply write timed out after 2 s (msg={msg:?})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_control_msg_rejects_oversize_length_prefix() {
        // Length prefix claims 5 MiB (> 4 MiB cap). Reader must bail
        // rather than allocate the buffer.
        let (mut a, mut b) = UnixStream::pair().unwrap();
        // Length = 5 MiB, as a 4-byte BE u32 split across `first_byte`
        // (0x00) + the 3-byte suffix `read_control_msg` reads itself.
        let len_bytes = (5u32 * 1024 * 1024).to_be_bytes();
        a.write_all(&len_bytes[1..]).await.unwrap();
        a.shutdown().await.unwrap();
        let result = read_control_msg(&mut b, len_bytes[0]).await;
        assert!(result.is_err(), "expected oversize rejection: {result:?}");
    }

    #[tokio::test]
    async fn read_control_msg_rejects_malformed_json() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let body = b"{not valid json";
        let len_buf = (body.len() as u32).to_be_bytes();
        a.write_all(&len_buf[1..]).await.unwrap();
        a.write_all(body).await.unwrap();
        a.shutdown().await.unwrap();
        let result = read_control_msg(&mut b, len_buf[0]).await;
        assert!(result.is_err(), "expected JSON parse error: {result:?}");
    }

    #[tokio::test]
    async fn read_control_msg_decodes_known_request() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let body = br#"{"type":"status"}"#;
        let len_buf = (body.len() as u32).to_be_bytes();
        a.write_all(&len_buf[1..]).await.unwrap();
        a.write_all(body).await.unwrap();
        a.shutdown().await.unwrap();
        let msg = read_control_msg(&mut b, len_buf[0]).await.unwrap();
        assert!(matches!(msg, ClientMsg::Status));
    }

    #[tokio::test]
    async fn read_control_msg_decodes_unknown_variant_for_forward_compat() {
        let (mut a, mut b) = UnixStream::pair().unwrap();
        let body = br#"{"type":"future_query"}"#;
        let len_buf = (body.len() as u32).to_be_bytes();
        a.write_all(&len_buf[1..]).await.unwrap();
        a.write_all(body).await.unwrap();
        a.shutdown().await.unwrap();
        let msg = read_control_msg(&mut b, len_buf[0]).await.unwrap();
        assert!(matches!(msg, ClientMsg::Unknown));
    }

    #[tokio::test]
    async fn start_listener_locks_socket_and_parent_dir_to_owner_only() {
        // Hard regression guard for the file-mode security contract
        // documented at `start_listener_at`. Any refactor that drops
        // either chmod silently exposes the attach channel to any
        // in-container uid sharing the agent uid — the exact threat
        // the comments name.
        let tmp = tempfile::tempdir().expect("tempdir");
        let parent = tmp.path().join("run");
        let socket_path = parent.join("jackin.sock");
        let _rx = start_listener_at(&socket_path).expect("bind");
        let parent_mode = std::fs::metadata(&parent)
            .expect("parent metadata")
            .permissions()
            .mode()
            & 0o777;
        let sock_mode = std::fs::metadata(&socket_path)
            .expect("socket metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            parent_mode, 0o700,
            "parent dir must be 0o700 (was {parent_mode:o})"
        );
        assert_eq!(sock_mode, 0o600, "socket must be 0o600 (was {sock_mode:o})");
    }
}
