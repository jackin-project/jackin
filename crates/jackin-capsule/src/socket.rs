//! Unix socket helpers: create, bind, and accept connections on
//! `/jackin/run/jackin.sock`.
//!
//! Not responsible for: protocol decoding, session management, or daemon
//! business logic.

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

type ClientPermit = tokio::sync::OwnedSemaphorePermit;
type ListenerReceiver = mpsc::UnboundedReceiver<(UnixStream, ClientPermit)>;
type ListenerWithLimiter = (ListenerReceiver, Arc<Semaphore>);

/// Bail out of the accept loop after this many consecutive errors. The
/// dominant cause of repeating `accept` failures is fd exhaustion
/// (EMFILE) — busy-looping at 100% CPU and flooding the docker-logs
/// stream is worse than tearing down the listener so PID 1 can exit
/// and the container restarts cleanly.
const ACCEPT_FAILURE_BAIL: u32 = 10;

/// Hard cap on concurrent attach connections. The socket is locked
/// 0600 so only the agent uid can dial in, but a rogue in-container
/// process that shares the agent uid can otherwise open thousands
/// of sockets — each a tokio task + `UnixStream` fd. Reject excess
/// connections by closing immediately so the legitimate operator's
/// attach is never starved.
const MAX_CONCURRENT_CLIENTS: usize = 16;

/// Start the Unix socket listener. Returns a receiver of newly-connected
/// clients paired with the concurrency-cap permit. The caller (daemon)
/// must hold the permit alive for the lifetime of the spawned attach
/// task so the per-process cap correctly tracks live connections.
pub fn start_listener() -> Result<ListenerReceiver> {
    start_listener_at(Path::new(SOCKET_PATH))
}

/// Bind a `UnixListener` at `path` with the parent dir locked to 0o700
/// and the socket file to 0o600. Test-visible variant: production
/// callers go through `start_listener`.
pub(crate) fn start_listener_at(path: &Path) -> Result<ListenerReceiver> {
    start_listener_at_with_limiter(path).map(|(rx, _limiter)| rx)
}

/// Same as `start_listener_at` but also returns the `Semaphore` Arc
/// the accept loop holds. Tests use the returned handle to assert on
/// `available_permits()` directly instead of relying on real-wall-
/// clock timeouts against `rx.recv()` for negative-delivery
/// assertions.
#[cfg(test)]
pub(crate) fn start_listener_at_with_limiter(path: &Path) -> Result<ListenerWithLimiter> {
    start_listener_at_inner(path)
}

#[cfg(not(test))]
fn start_listener_at_with_limiter(path: &Path) -> Result<ListenerWithLimiter> {
    start_listener_at_inner(path)
}

fn start_listener_at_inner(path: &Path) -> Result<ListenerWithLimiter> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("removing stale socket {}", path.display()));
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        // Parent dir 0o700 so only the owner can list/connect. The socket
        // file itself gets 0o600 after bind, but on a system where the
        // parent dir is world-x an attacker can still enumerate the path.
        // Lock both. The dir is host-owned and the capsule runs as that
        // same UID (`--user` on docker run), so the owner can set this.
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("locking socket parent {} to 0o700", parent.display()))?;
    }

    let listener = UnixListener::bind(path)?;
    // Lock the socket to owner-only. Without this, any in-container
    // process that shares the agent uid (and any process running as a
    // different uid if the umask is generous) can connect and inject
    // `ClientFrame::Input` straight into the focused PTY. The attach
    // channel has no authentication beyond file-mode. Hard error: the
    // capsule always owns the socket file it just created.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .with_context(|| format!("locking socket {} to 0o600", path.display()))?;
    let (tx, rx) = mpsc::unbounded_channel();
    let limiter = Arc::new(Semaphore::new(MAX_CONCURRENT_CLIENTS));
    let limiter_for_task = Arc::clone(&limiter);

    tokio::spawn(async move {
        let limiter = limiter_for_task;
        let mut consecutive_failures = 0u32;
        // `true` while the semaphore is fully acquired. Used to log
        // the saturation transition exactly once instead of once per
        // dropped over-cap connection — a flood attacker (the exact
        // threat the cap defends against) would otherwise drown the
        // compact log tier in repeated drop lines, masking other
        // lifecycle events.
        let mut at_cap_logged = false;
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
                    let permit = if let Ok(p) = Arc::clone(&limiter).try_acquire_owned() {
                        if at_cap_logged {
                            crate::clog!(
                                "socket: capacity recovered below cap {MAX_CONCURRENT_CLIENTS}"
                            );
                            at_cap_logged = false;
                        }
                        p
                    } else {
                        if at_cap_logged {
                            crate::cdebug!(
                                "socket: dropping over-cap connection (cap={MAX_CONCURRENT_CLIENTS})"
                            );
                        } else {
                            crate::clog!(
                                "socket: at concurrent-client cap {MAX_CONCURRENT_CLIENTS}; over-cap connections will be dropped silently until capacity recovers"
                            );
                            at_cap_logged = true;
                        }
                        drop(stream);
                        continue;
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
                    // Backoff timing is mechanical detail — the
                    // accept-error clog above already names the
                    // failure. Keep this on the debug tier so the
                    // 1-line-per-failure compact-log invariant holds.
                    crate::cdebug!("socket: backing off {backoff_ms}ms before next accept");
                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                }
            }
        }
    });

    Ok((rx, limiter))
}

/// Maximum wall-clock time for a single control-channel read. Capped
/// so a peer that stalls between length prefix and body — or trickles
/// bytes slower than the bandwidth bound — cannot pin a tokio task
/// and its concurrency permit indefinitely. 4 MiB across 10 s is
/// ~409 KiB/s, well below any reasonable localhost rate.
const CONTROL_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Read a length-prefixed JSON control message whose 4-byte length
/// prefix begins with `first_byte = 0x00` (already consumed by the
/// dispatcher). Returns `Err` with context on every failure mode so
/// callers can clog the underlying cause; a silently-collapsed None
/// would let the host's `jackin status` block on `read_exact` for a
/// reply that never comes.
pub async fn read_control_msg(stream: &mut UnixStream, first_byte: u8) -> Result<ClientMsg> {
    let mut rest = [0u8; 3];
    tokio::time::timeout(CONTROL_READ_TIMEOUT, stream.read_exact(&mut rest))
        .await
        .context("control msg: timed out reading length suffix")?
        .context("control msg: reading length suffix")?;
    let len_buf = [first_byte, rest[0], rest[1], rest[2]];
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_CONTROL_MSG: usize = 4 * 1024 * 1024;
    if len > MAX_CONTROL_MSG {
        anyhow::bail!("control msg length {len} exceeds limit {MAX_CONTROL_MSG}");
    }
    let body = read_payload_lazy(stream, len, CONTROL_READ_TIMEOUT)
        .await
        .context("control msg: reading body")?;
    serde_json::from_slice(&body).context("control msg: parsing JSON body")
}

/// Read exactly `len` bytes from `stream` into a freshly-allocated
/// `Vec<u8>` that grows lazily as chunks arrive. Two reasons for the
/// chunked shape over `read_exact(&mut vec![0u8; len])`:
///
/// 1. `vec![0u8; len]` calls `write_bytes(0)` over `len` bytes before
///    any payload byte arrives. A peer that sends a 4 MiB length
///    prefix then stalls forces 4 MiB of zero-touched RSS per
///    connection. `Vec::with_capacity` only reserves and grows as
///    `extend_from_slice` runs, so memset cost scales with bytes
///    actually delivered.
/// 2. Bounded `total_timeout` for the whole read prevents a
///    trickle-bytes attacker from holding the connection (and the
///    attach-concurrency permit) indefinitely.
async fn read_payload_lazy(
    stream: &mut UnixStream,
    len: usize,
    total_timeout: Duration,
) -> Result<Vec<u8>> {
    let mut buf: Vec<u8> = Vec::with_capacity(len.min(64 * 1024));
    let mut remaining = len;
    let mut chunk = [0u8; 16 * 1024];
    while remaining > 0 {
        let n = chunk.len().min(remaining);
        tokio::time::timeout(total_timeout, stream.read_exact(&mut chunk[..n]))
            .await
            .context("read timed out")?
            .context("short read")?;
        buf.extend_from_slice(&chunk[..n]);
        remaining -= n;
    }
    Ok(buf)
}

/// Handle a one-shot control request and close the connection.
/// A runtime hook/plugin event forwarded over the control socket, handed to the
/// daemon loop to apply to the addressed session's authority.
#[derive(Debug, Clone)]
pub struct RuntimeEventMsg {
    pub session_id: u64,
    pub source_id: String,
    pub runtime: String,
    pub event: String,
}

pub async fn handle_control_request(
    mut stream: UnixStream,
    first_byte: u8,
    sessions: Vec<SessionInfo>,
    tabs: Vec<crate::protocol::control::TabSnapshot>,
    history: Vec<jackin_protocol::control::AgentRegistryEntry>,
    active_tab: u32,
    runtime_event_tx: mpsc::UnboundedSender<RuntimeEventMsg>,
) {
    let msg = match read_control_msg(&mut stream, first_byte).await {
        Ok(msg) => msg,
        Err(e) => {
            crate::clog!("control: rejecting malformed request: {e:#}");
            return;
        }
    };
    let reply = match &msg {
        ClientMsg::Status => ServerMsg::SessionList { sessions },
        ClientMsg::Snapshot => ServerMsg::Snapshot { tabs, active_tab },
        ClientMsg::Agents => ServerMsg::AgentRegistry { records: history },
        ClientMsg::ReportRuntimeEvent {
            session_id,
            source_id,
            runtime,
            event,
            payload: _,
        } => {
            // Forward to the daemon loop; never block the agent's hook.
            if runtime_event_tx
                .send(RuntimeEventMsg {
                    session_id: *session_id,
                    source_id: source_id.clone(),
                    runtime: runtime.clone(),
                    event: event.clone(),
                })
                .is_err()
            {
                crate::clog!("control: runtime event dropped (daemon loop gone)");
            }
            ServerMsg::Ack
        }
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
mod tests;
