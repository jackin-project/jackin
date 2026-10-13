// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container-local usage socket bridged over a host-started stdio tunnel.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, UsageBrokerRequest, UsageBrokerResponse, UsageCoordinationError,
    UsageCoordinationErrorKind, UsageRelayTunnelRequest, UsageRelayTunnelResponse,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWrite,
    AsyncWriteExt as _, BufReader,
};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc, oneshot};

mod authorization;
mod identity;

pub(crate) use authorization::UsageRelayAuthorization;
#[cfg(test)]
pub(crate) use identity::{DEFAULT_CAPSULE_SUPERVISOR_PID, parse_supervisor_pid};
pub(crate) use identity::{
    PeerIdentity, SupervisorIdentity, load_supervisor_identity, peer_identity,
    supervisor_peer_allows,
};
#[cfg(all(test, target_os = "linux"))]
pub(crate) use identity::{parse_proc_stat_start_time, process_start_time};

const TUNNEL_CAPACITY: usize = 128;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(35);

type Pending = Arc<Mutex<BTreeMap<u64, oneshot::Sender<UsageBrokerResponse>>>>;

/// Bind the Capsule-local scoped usage socket and bridge requests over stdio.
pub(crate) async fn run() -> Result<()> {
    let config = crate::config::load().context("loading Capsule config for usage relay")?;
    let authorization = UsageRelayAuthorization::from_config(&config)
        .context("building usage relay session authorization")?;
    run_at(
        Path::new(jackin_core::container_paths::USAGE_SOCK),
        load_supervisor_identity()?,
        authorization,
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await
}

async fn run_at<R, W>(
    socket_path: &Path,
    supervisor: SupervisorIdentity,
    authorization: UsageRelayAuthorization,
    input: R,
    output: W,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    run_at_with_peer(socket_path, supervisor, authorization, None, input, output).await
}

async fn run_at_with_peer<R, W>(
    socket_path: &Path,
    supervisor: SupervisorIdentity,
    authorization: UsageRelayAuthorization,
    forced_peer: Option<PeerIdentity>,
    input: R,
    output: W,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    drop(std::fs::remove_file(socket_path));
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "creating scoped usage socket directory {}",
                parent.display()
            )
        })?;
    }
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("binding scoped usage socket at {}", socket_path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600))?;
    }
    let _cleanup = SocketCleanup(socket_path.to_path_buf());
    let authorization = Arc::new(authorization);
    let pending: Pending = Arc::new(Mutex::new(BTreeMap::new()));
    let request_ids = Arc::new(AtomicU64::new(1));
    let (requests, mut request_rx) = mpsc::channel::<UsageRelayTunnelRequest>(TUNNEL_CAPACITY);

    let mut writer = jackin_telemetry::spawn::spawn_stream("usage_relay.writer", async move {
        let mut output = output;
        while let Some(request) = request_rx.recv().await {
            write_frame(&mut output, &request).await?;
        }
        Ok::<(), anyhow::Error>(())
    });
    let response_pending = Arc::clone(&pending);
    let mut reader = jackin_telemetry::spawn::spawn_stream("usage_relay.reader", async move {
        let mut input = BufReader::new(input);
        loop {
            let response = read_frame::<_, UsageRelayTunnelResponse>(&mut input).await?;
            if let Some(waiter) = response_pending.lock().await.remove(&response.request_id) {
                drop(waiter.send(response.response));
            }
        }
    });

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let requests = requests.clone();
                let pending = Arc::clone(&pending);
                let authorization = Arc::clone(&authorization);
                let peer = forced_peer.or_else(|| peer_identity(&stream));
                let request_id = request_ids.fetch_add(1, Ordering::Relaxed);
                drop(jackin_telemetry::spawn::spawn_stream(
                    "usage_relay.local_request",
                    handle_local(
                        stream,
                        request_id,
                        requests,
                        pending,
                        supervisor,
                        authorization,
                        peer,
                    ),
                ));
            }
            result = &mut reader => {
                fail_pending(&pending).await;
                return result.context("usage relay response task panicked")?;
            }
            result = &mut writer => {
                fail_pending(&pending).await;
                return result.context("usage relay request task panicked")?;
            }
        }
    }
}

async fn handle_local(
    mut stream: UnixStream,
    request_id: u64,
    requests: mpsc::Sender<UsageRelayTunnelRequest>,
    pending: Pending,
    supervisor: SupervisorIdentity,
    authorization: Arc<UsageRelayAuthorization>,
    peer: Option<PeerIdentity>,
) {
    let request = {
        let mut reader = BufReader::new(&mut stream);
        read_frame::<_, UsageBrokerRequest>(&mut reader).await
    };
    let supervisor_ok = supervisor_peer_allows(supervisor, peer);
    let response = match request {
        Ok(request)
            if supervisor_ok && authorization.authorizes(supervisor, peer, &request.operation) =>
        {
            let (response_tx, response_rx) = oneshot::channel();
            pending.lock().await.insert(request_id, response_tx);
            let tunneled = UsageRelayTunnelRequest {
                request_id,
                request,
            };
            if requests.send(tunneled).await.is_err() {
                pending.lock().await.remove(&request_id);
                unavailable_response()
            } else if let Ok(Ok(response)) =
                tokio::time::timeout(RESPONSE_TIMEOUT, response_rx).await
            {
                response
            } else {
                pending.lock().await.remove(&request_id);
                unavailable_response()
            }
        }
        Ok(_) if supervisor_ok => capability_unauthorized_response(),
        Ok(_) => supervisor_unauthorized_response(),
        Err(_) => protocol_response(),
    };
    drop(write_frame(&mut stream, &response).await);
}

fn capability_unauthorized_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "usage account capability is not authorized".to_owned(),
        },
    }
}

async fn fail_pending(pending: &Pending) {
    let waiters = std::mem::take(&mut *pending.lock().await);
    for (_, waiter) in waiters {
        drop(waiter.send(unavailable_response()));
    }
}

async fn read_frame<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncBufRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut bytes = Vec::new();
    let read = reader
        .take(u64::try_from(USAGE_BROKER_MAX_FRAME_BYTES).unwrap_or(u64::MAX) + 1)
        .read_until(b'\n', &mut bytes)
        .await?;
    if read == 0 || read > USAGE_BROKER_MAX_FRAME_BYTES || bytes.last() != Some(&b'\n') {
        bail!("usage relay frame is invalid");
    }
    bytes.pop();
    serde_json::from_slice(&bytes).context("decoding usage relay frame")
}

async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() >= USAGE_BROKER_MAX_FRAME_BYTES {
        bail!("usage relay frame is too large");
    }
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

fn unavailable_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unavailable,
            message: "usage broker is unavailable".to_owned(),
        },
    }
}

fn protocol_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::ProtocolMismatch,
            message: "usage relay protocol mismatch".to_owned(),
        },
    }
}

fn supervisor_unauthorized_response() -> UsageBrokerResponse {
    UsageBrokerResponse::Error {
        error: UsageCoordinationError {
            kind: UsageCoordinationErrorKind::Unauthorized,
            message: "usage relay peer is not the Capsule supervisor".to_owned(),
        },
    }
}

struct SocketCleanup(std::path::PathBuf);

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.0));
    }
}

#[cfg(test)]
mod tests;
