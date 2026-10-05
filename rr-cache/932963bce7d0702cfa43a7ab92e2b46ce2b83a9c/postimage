// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Container-local usage socket bridged over a host-started stdio tunnel.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use jackin_protocol::usage_broker::{
    USAGE_BROKER_MAX_FRAME_BYTES, UsageAccountCapability, UsageBrokerOperation, UsageBrokerRequest,
    UsageBrokerResponse, UsageCoordinationError, UsageCoordinationErrorKind,
    UsageRelayTunnelRequest, UsageRelayTunnelResponse,
};
use jackin_protocol::{CapsuleConfig, SessionIdentity};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt as _, AsyncRead, AsyncReadExt as _, AsyncWrite,
    AsyncWriteExt as _, BufReader,
};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc, oneshot};

const TUNNEL_CAPACITY: usize = 128;
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(35);
const DEFAULT_CAPSULE_SUPERVISOR_PID: u32 = 1;

type Pending = Arc<Mutex<BTreeMap<u64, oneshot::Sender<UsageBrokerResponse>>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PeerIdentity {
    pid: Option<u32>,
    uid: u32,
    gid: u32,
}

/// Immutable capability binding loaded from the host-validated Capsule config.
/// Session peers get exactly one capability through their kernel UID/GID; the
/// root Capsule supervisor may use the launch-wide set for daemon refreshes.
#[derive(Debug, Clone, Default)]
struct UsageRelayAuthorization {
    by_peer: BTreeMap<(u32, u32), UsageAccountCapability>,
    launch_capabilities: BTreeSet<UsageAccountCapability>,
}

impl UsageRelayAuthorization {
    fn from_config(config: &CapsuleConfig) -> Result<Self> {
        let mut authorization = Self::default();
        for (instance, capability) in &config.usage_capabilities {
            anyhow::ensure!(
                config
                    .instances
                    .iter()
                    .any(|candidate| candidate == instance),
                "usage capability names an instance outside the configured allowlist"
            );
            anyhow::ensure!(
                !capability.account_id.is_empty() && !capability.surface_id.is_empty(),
                "usage capability for instance {instance:?} is empty"
            );
            let identity = config.identity_for_instance(instance).ok_or_else(|| {
                anyhow::anyhow!("usage instance {instance:?} has no Unix identity")
            })?;
            let peer = PeerIdentity::from(identity);
            anyhow::ensure!(
                authorization
                    .by_peer
                    .insert((peer.uid, peer.gid), capability.clone())
                    .is_none(),
                "multiple usage instances share Unix identity {peer:?}"
            );
            authorization.launch_capabilities.insert(capability.clone());
        }
        Ok(authorization)
    }

    fn authorizes(
        &self,
        peer: Option<PeerIdentity>,
        operation: &UsageBrokerOperation,
        supervisor_pid: u32,
    ) -> bool {
        let Some(capability) = operation_capability(operation) else {
            return false;
        };
        let Some(peer) = peer else {
            return false;
        };
        if peer.uid == 0 && peer.gid == 0 && peer.pid == Some(supervisor_pid) {
            return self.launch_capabilities.contains(capability);
        }
        self.by_peer.get(&(peer.uid, peer.gid)) == Some(capability)
    }

    #[cfg(test)]
    fn for_peer(peer: PeerIdentity, capability: UsageAccountCapability) -> Self {
        Self {
            by_peer: BTreeMap::from([((peer.uid, peer.gid), capability.clone())]),
            launch_capabilities: BTreeSet::from([capability]),
        }
    }
}

impl From<SessionIdentity> for PeerIdentity {
    fn from(identity: SessionIdentity) -> Self {
        Self {
            pid: None,
            uid: identity.uid,
            gid: identity.gid,
        }
    }
}

fn operation_capability(operation: &UsageBrokerOperation) -> Option<&UsageAccountCapability> {
    match operation {
        UsageBrokerOperation::CurrentForCapability { capability }
        | UsageBrokerOperation::RefreshForCapability { capability, .. }
        | UsageBrokerOperation::JoinForCapability { capability, .. }
        | UsageBrokerOperation::Current { capability }
        | UsageBrokerOperation::Refresh { capability, .. }
        | UsageBrokerOperation::Join { capability, .. } => Some(capability),
        UsageBrokerOperation::CurrentProjection
        | UsageBrokerOperation::RequestRefresh { .. }
        | UsageBrokerOperation::JoinPublication { .. }
        | UsageBrokerOperation::ReconcileCatalog { .. }
        | UsageBrokerOperation::CurrentProjectionForSurface
        | UsageBrokerOperation::RequestRefreshForSurface { .. }
        | UsageBrokerOperation::JoinPublicationForSurface { .. } => None,
    }
}

/// Bind the Capsule-local scoped usage socket and bridge requests over stdio.
pub(crate) async fn run() -> Result<()> {
    let config = crate::config::load().context("loading Capsule config for usage relay")?;
    let authorization = UsageRelayAuthorization::from_config(&config)
        .context("building usage relay session authorization")?;
    let supervisor_pid = load_supervisor_pid()?;
    run_at(
        Path::new(jackin_core::container_paths::USAGE_SOCK),
        authorization,
        supervisor_pid,
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await
}

async fn run_at<R, W>(
    socket_path: &Path,
    authorization: UsageRelayAuthorization,
    supervisor_pid: u32,
    input: R,
    output: W,
) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    run_at_with_peer(
        socket_path,
        authorization,
        supervisor_pid,
        None,
        input,
        output,
    )
    .await
}

fn load_supervisor_pid() -> Result<u32> {
    parse_supervisor_pid(std::env::var(jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV))
}

fn parse_supervisor_pid(variable: Result<String, std::env::VarError>) -> Result<u32> {
    let supervisor_pid = match variable {
        Ok(value) => value.parse::<u32>().with_context(|| {
            format!(
                "invalid {} value {value:?}",
                jackin_protocol::CAPSULE_SUPERVISOR_PID_ENV
            )
        })?,
        Err(std::env::VarError::NotPresent) => DEFAULT_CAPSULE_SUPERVISOR_PID,
        Err(error) => return Err(error.into()),
    };
    anyhow::ensure!(
        supervisor_pid > 0,
        "Capsule supervisor PID must be positive"
    );
    Ok(supervisor_pid)
}

fn supervisor_peer_allows(supervisor_pid: u32, peer: Option<PeerIdentity>) -> bool {
    let Some(peer) = peer else {
        return false;
    };
    if peer.uid == 0 || peer.gid == 0 {
        return peer.uid == 0 && peer.gid == 0 && peer.pid == Some(supervisor_pid);
    }
    true
}

fn peer_identity(stream: &UnixStream) -> Option<PeerIdentity> {
    stream.peer_cred().ok().map(|credentials| PeerIdentity {
        pid: credentials.pid().and_then(|pid| u32::try_from(pid).ok()),
        uid: credentials.uid(),
        gid: credentials.gid(),
    })
}

async fn run_at_with_peer<R, W>(
    socket_path: &Path,
    authorization: UsageRelayAuthorization,
    supervisor_pid: u32,
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
                        authorization,
                        supervisor_pid,
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
    authorization: Arc<UsageRelayAuthorization>,
    supervisor_pid: u32,
    peer: Option<PeerIdentity>,
) {
    let request = {
        let mut reader = BufReader::new(&mut stream);
        read_frame::<_, UsageBrokerRequest>(&mut reader).await
    };
    let response = match request {
        Ok(request)
            if supervisor_peer_allows(supervisor_pid, peer)
                && authorization.authorizes(peer, &request.operation, supervisor_pid) =>
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
        Ok(_) => unauthorized_response(),
        Err(_) => protocol_response(),
    };
    drop(write_frame(&mut stream, &response).await);
}

fn unauthorized_response() -> UsageBrokerResponse {
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

fn unauthorized_response() -> UsageBrokerResponse {
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
