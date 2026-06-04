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
/// of sockets — each a tokio task + UnixStream fd. Reject excess
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
                    let permit = match limiter.clone().try_acquire_owned() {
                        Ok(p) => {
                            if at_cap_logged {
                                crate::clog!(
                                    "socket: capacity recovered below cap {MAX_CONCURRENT_CLIENTS}"
                                );
                                at_cap_logged = false;
                            }
                            p
                        }
                        Err(_) => {
                            if !at_cap_logged {
                                crate::clog!(
                                    "socket: at concurrent-client cap {MAX_CONCURRENT_CLIENTS}; over-cap connections will be dropped silently until capacity recovers"
                                );
                                at_cap_logged = true;
                            } else {
                                crate::cdebug!(
                                    "socket: dropping over-cap connection (cap={MAX_CONCURRENT_CLIENTS})"
                                );
                            }
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
///
/// State-mutating messages (`ReportAgentState`, `HeartbeatAgentAuthority`,
/// `ClearAgentAuthority`) are forwarded through `control_msg_tx` to the
/// daemon's main event loop for processing; no reply is written for those.
pub async fn handle_control_request(
    mut stream: UnixStream,
    first_byte: u8,
    sessions: Vec<SessionInfo>,
    tabs: Vec<crate::protocol::control::TabSnapshot>,
    active_tab: u32,
    control_msg_tx: mpsc::UnboundedSender<ClientMsg>,
    state_broadcast_tx: tokio::sync::broadcast::Sender<ServerMsg>,
) {
    let msg = match read_control_msg(&mut stream, first_byte).await {
        Ok(msg) => msg,
        Err(e) => {
            crate::clog!("control: rejecting malformed request: {e:#}");
            return;
        }
    };
    // State-mutating messages are forwarded to the daemon's main event loop
    // rather than handled inline; they need no reply.
    if matches!(
        msg,
        ClientMsg::ReportAgentState { .. }
            | ClientMsg::HeartbeatAgentAuthority { .. }
            | ClientMsg::ClearAgentAuthority { .. }
            | ClientMsg::ReportChildAgentState { .. }
    ) {
        let _ = control_msg_tx.send(msg);
        return;
    }
    let reply = match msg {
        ClientMsg::Status => ServerMsg::SessionList { sessions },
        ClientMsg::Snapshot => ServerMsg::Snapshot { tabs, active_tab },
        ClientMsg::Unknown => {
            // Reply with `Unknown` so the peer's `read_exact` returns
            // immediately rather than hanging until SOCKET_TIMEOUT.
            crate::clog!("control: ignoring unknown ClientMsg variant from peer");
            ServerMsg::Unknown
        }
        ClientMsg::WaitSessionStatus {
            session_id,
            ref target_statuses,
            timeout_ms,
        } => {
            let timeout_dur =
                std::time::Duration::from_millis(timeout_ms.unwrap_or(30_000));
            let current = sessions
                .iter()
                .find(|s| s.id == session_id)
                .map(|s| s.state.label().to_string());
            match current {
                None => {
                    crate::cdebug!(
                        "session {session_id}: WaitSessionStatus outcome=not_found"
                    );
                    ServerMsg::SessionStatusResult {
                        session_id,
                        effective: "unknown".to_string(),
                        revision: 0,
                        outcome: "not_found".to_string(),
                    }
                }
                Some(ref cur) if target_statuses.contains(cur) => {
                    crate::cdebug!(
                        "session {session_id}: WaitSessionStatus outcome=satisfied effective={cur}"
                    );
                    ServerMsg::SessionStatusResult {
                        session_id,
                        effective: cur.clone(),
                        revision: 0,
                        outcome: "satisfied".to_string(),
                    }
                }
                Some(ref cur) => {
                    // Not yet satisfied — subscribe to broadcast and wait.
                    let mut rx = state_broadcast_tx.subscribe();
                    let deadline = tokio::time::Instant::now() + timeout_dur;
                    let cur = cur.clone();
                    let targets = target_statuses.clone();
                    loop {
                        let rem = deadline
                            .saturating_duration_since(tokio::time::Instant::now());
                        if rem.is_zero() {
                            crate::cdebug!(
                                "session {session_id}: WaitSessionStatus outcome=timeout effective={cur}"
                            );
                            break ServerMsg::SessionStatusResult {
                                session_id,
                                effective: cur,
                                revision: 0,
                                outcome: "timeout".to_string(),
                            };
                        }
                        match tokio::time::timeout(rem, rx.recv()).await {
                            Ok(Ok(ServerMsg::AgentStateChanged {
                                session_id: esid,
                                ref effective,
                                revision,
                                ..
                            })) if esid == session_id && targets.contains(effective) => {
                                crate::cdebug!(
                                    "session {session_id}: WaitSessionStatus outcome=satisfied effective={effective}"
                                );
                                break ServerMsg::SessionStatusResult {
                                    session_id,
                                    effective: effective.clone(),
                                    revision,
                                    outcome: "satisfied".to_string(),
                                };
                            }
                            Ok(Ok(_)) => continue,
                            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {
                                continue;
                            }
                            _ => {
                                crate::cdebug!(
                                    "session {session_id}: WaitSessionStatus outcome=timeout (channel closed)"
                                );
                                break ServerMsg::SessionStatusResult {
                                    session_id,
                                    effective: cur.clone(),
                                    revision: 0,
                                    outcome: "timeout".to_string(),
                                };
                            }
                        }
                    }
                }
            }
        }
        ClientMsg::SessionReadVisible { session_id, .. } => {
            // Visible text read is not yet implemented; return empty lines.
            ServerMsg::SessionVisibleText {
                session_id,
                lines: vec![],
            }
        }
        ClientMsg::TokenGetSession { session_id } => {
            let token_usage = sessions
                .iter()
                .find(|s| s.id == session_id)
                .and_then(|s| s.token_usage.clone());
            ServerMsg::TokenSessionResult {
                session_id,
                token_usage,
            }
        }
        ClientMsg::TokenGetModels { .. } => ServerMsg::TokenModelsResult {
            provider: "claude".to_string(),
            models: vec![
                "claude-opus-4-8-20251101".to_string(),
                "claude-sonnet-4-6-20251101".to_string(),
                "claude-haiku-4-5-20251001".to_string(),
            ],
        },
        ClientMsg::EventsSubscribe { subscriber_id } => {
            crate::clog!(
                "events.subscribe: new subscriber {:?}",
                subscriber_id.as_deref().unwrap_or("anon")
            );
            let welcome = ServerMsg::Welcome {
                jackin_protocol_version: "1".to_string(),
            };
            if stream.write_all(&frame(&welcome)).await.is_err() {
                return;
            }
            let mut rx = state_broadcast_tx.subscribe();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if stream.write_all(&frame(&event)).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        crate::clog!("events.subscribe: subscriber lagged {n} events; continuing");
                        continue;
                    }
                }
            }
            return;
        }
        _ => {
            crate::clog!("control: unhandled ClientMsg variant in one-shot handler");
            ServerMsg::Unknown
        }
    };
    // Bound the reply write so a peer that disappeared between request
    // decode and reply write cannot wedge this task forever holding the
    // attach-concurrency permit. 2 s is generous for a single localhost
    // socket write; anything slower is the peer being unresponsive.
    match tokio::time::timeout(Duration::from_secs(2), stream.write_all(&frame(&reply))).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => crate::clog!("control reply write failed: {e}"),
        Err(_) => crate::clog!("control reply write timed out after 2 s"),
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
    async fn start_listener_caps_concurrent_clients_at_max() {
        // Hard regression guard for `MAX_CONCURRENT_CLIENTS`. Without
        // the cap, any in-uid process can flood the attach channel
        // and starve the legitimate operator. The over-cap connection
        // must drop on the server side without ever landing in `rx`.
        //
        // Negative-delivery assertions go through `limiter`
        // directly (`available_permits == 0` after saturation) rather
        // than real-wall-clock `timeout()` checks against `rx.recv()`
        // — the wall-clock approach passed on loaded CI runners
        // simply because the daemon hadn't been scheduled within the
        // timeout window, masking real cap regressions. Reading the
        // semaphore is cap-sensitive instead of timing-sensitive.
        let tmp = tempfile::tempdir().expect("tempdir");
        let parent = tmp.path().join("run");
        let socket_path = parent.join("jackin.sock");
        let (mut rx, limiter) = start_listener_at_with_limiter(&socket_path).expect("bind");

        // Hold every accepted stream + permit so the semaphore stays
        // saturated. Dropping the permit would let the next accept
        // proceed and invalidate the assertion.
        //
        // Per-iteration `connect().await` then `rx.recv()` assumes
        // the unbounded mpsc preserves FIFO order of accepts — held
        // today and contract-stable across tokio versions.
        let mut held: Vec<(UnixStream, tokio::sync::OwnedSemaphorePermit)> = Vec::new();
        let mut client_streams: Vec<UnixStream> = Vec::new();
        for i in 0..MAX_CONCURRENT_CLIENTS {
            let client = UnixStream::connect(&socket_path)
                .await
                .unwrap_or_else(|e| panic!("connect {i}: {e}"));
            client_streams.push(client);
            let pair = tokio::time::timeout(Duration::from_secs(2), rx.recv())
                .await
                .unwrap_or_else(|_| panic!("rx did not deliver connection {i}"))
                .expect("rx closed");
            held.push(pair);
        }
        assert_eq!(
            limiter.available_permits(),
            0,
            "after saturating the cap, no permits should remain"
        );

        // Cap is now at MAX. The next connect should be accepted by
        // the kernel but dropped on the server side. Yield to the
        // tokio scheduler so the accept loop processes the over-cap
        // connect, then check the semaphore: it must still report 0
        // (no permit acquired) because `try_acquire_owned` failed and
        // the loop continued without delivering to `rx`.
        let over_cap_client = UnixStream::connect(&socket_path)
            .await
            .expect("kernel-side connect");
        client_streams.push(over_cap_client);
        for _ in 0..10 {
            tokio::task::yield_now().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            limiter.available_permits(),
            0,
            "over-cap connect must not consume a permit"
        );
        match rx.try_recv() {
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
            other => panic!("rx must not deliver beyond MAX_CONCURRENT_CLIENTS; got: {other:?}"),
        }

        // Releasing one permit must let a fresh attach through.
        drop(held.pop().expect("drop one held permit"));
        let new_client = UnixStream::connect(&socket_path)
            .await
            .expect("post-release connect");
        client_streams.push(new_client);
        let resumed = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("rx did not resume after permit release")
            .expect("rx closed");
        held.push(resumed);
        assert_eq!(
            limiter.available_permits(),
            0,
            "after re-saturation the cap should hold permits at 0"
        );
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
