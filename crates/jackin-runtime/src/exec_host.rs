// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Host-side credential resolver for `jackin-exec`.
//!
//! Listens on a Unix socket at `~/.jackin/sockets/<container>/host.sock`
//! which is bind-mounted into the role container at `/jackin/run/host.sock`.
//! When the capsule daemon confirms an `ExecCommand` and the operator has
//! selected credentials in the picker, the capsule connects here to resolve
//! the on-demand env vars before running the command.
//!
//! The `jackin load` process stays alive for the session and this listener
//! runs as a `tokio::spawn` task alongside the blocking interactive attach.
//! Future work: migrate to the jackin❯ daemon so all running containers share
//! one host-side resolver.
//!
//! # Security
//!
//! The listener validates every incoming resolution request against the
//! `allowed_bindings` set configured at session start. Only (name, kind,
//! source) triples that exactly match an operator-configured binding are
//! resolved. Unknown refs are rejected with a `CredReply::Error` without
//! calling `op` or reading any host env var. This prevents a compromised in-container
//! process from requesting arbitrary secret resolution via the host socket.
//!
//! For `kind = "op"`, `source` must start with `op://` and the `--`
//! end-of-options sentinel is inserted before passing to `op read` to prevent
//! argument injection via crafted op:// values.
//!
//! On Linux, the listener also authenticates the socket peer with safe
//! `SO_PEERCRED` (`UnixStream::peer_cred`) and accepts only the container's
//! launch-owned init PID and UID, with a pinned process start time and PID
//! namespace, local kernel cgroup ownership by the immutable Docker ID,
//! plus an `NSpid` vector exactly `[host_pid, 1]`. This requires
//! the one container PID namespace directly hosted by the launch process and
//! rejects nested PID namespaces. That binds credential resolution to the
//! daemon path that already enforces the operator picker. Non-Linux hosts fail
//! closed: the relay is disabled until an equivalent peer-identity mechanism
//! is implemented for that backend.
//! File permissions and an operator binding allowlist are not a substitute
//! for authenticating the in-container caller.

use anyhow::{Context as _, Result};
use jackin_protocol::control::frame;
use jackin_protocol::{CredReply, CredRequest, ExecBinding, ExecKind};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// Start a relay bound to the immutable container created by this launch.
/// Runtime inspection brackets capture so PID recycling during capture fails closed.
pub async fn start_for_container(
    docker: &impl jackin_docker::docker_client::DockerApi,
    jackin_home: &Path,
    container: &jackin_docker::docker_client::ContainerHandle,
    exec_bindings: &[ExecBinding],
) -> Result<tokio::task::JoinHandle<()>> {
    ensure_caller_auth_supported()?;
    let init_pid = docker.container_init_pid_by_id(container).await?;
    let identity = CapsulePeerIdentity::capture(container, init_pid)?;
    anyhow::ensure!(docker.container_init_pid_by_id(container).await? == init_pid,
        "container init changed during credential relay authentication");
    let caller_auth = CallerAuth::CapsuleDaemon(identity);
    let sock_path = jackin_home.join("sockets").join(container.name()).join("host.sock");
    let listener = bind_listener(&sock_path)?;
    let allowed_bindings = exec_bindings.to_vec();
    Ok(jackin_telemetry::spawn::spawn_stream("exec_host.connection", async move {
        drop(run_bound_listener(listener, &allowed_bindings, caller_auth).await);
    }))
}

#[derive(Clone, Debug)]
enum CallerAuth {
    CapsuleDaemon(CapsulePeerIdentity),
    #[cfg(all(test, target_os = "linux"))]
    PeerPid(u32),
    #[cfg(all(test, not(target_os = "linux")))]
    TestPeer,
}

/// Ensure the host can authenticate the capsule daemon before enabling a
/// credential relay with configured bindings.
pub(crate) fn ensure_caller_auth_supported() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        anyhow::bail!(
            "host credential relay is disabled on non-Linux hosts: capsule daemon peer authentication is unavailable"
        )
    }
}

async fn run_bound_listener(
    listener: UnixListener,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
) -> Result<()> {
    let _close = jackin_telemetry::stream::close_on_drop();

    loop {
        if let Ok((stream, _)) = listener.accept().await {
            if handle_connection(stream, allowed_bindings, caller_auth.clone())
                .await
                .is_err()
            {
                let _error = jackin_telemetry::record_error(
                    jackin_telemetry::schema::enums::ErrorType::RpcError,
                );
            }
        } else {
            let _error =
                jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::IoError);
            let _retry = jackin_telemetry::record_retry_scheduled();
            // Brief back-off to avoid tight loop on persistent errors.
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}

fn bind_listener(sock_path: &Path) -> Result<UnixListener> {
    // Remove stale socket from a previous session.
    drop(std::fs::remove_file(sock_path));
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
        // host.sock is the credential-resolution boundary. Launch creates this
        // directory at 0o700 before writing container-visible config; repeat
        // the restriction here so independently started listeners preserve the
        // same operator-only boundary.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    UnixListener::bind(sock_path)
        .with_context(|| format!("binding host.sock at {}", sock_path.display()))
}

async fn handle_connection(
    stream: UnixStream,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
) -> Result<()> {
    handle_connection_with_resolver(stream, allowed_bindings, caller_auth, |refs| async move {
        resolve_all(&refs).await
    }).await
}

async fn handle_connection_with_resolver<F, Fut>(
    mut stream: UnixStream,
    allowed_bindings: &[ExecBinding],
    caller_auth: CallerAuth,
    resolve: F,
) -> Result<()>
where
    F: FnOnce(Vec<ExecBinding>) -> Fut,
    Fut: Future<Output = Result<std::collections::BTreeMap<String, String>>>,
{
    const MAX_REQ: usize = 512 * 1024;
    let attrs = [
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_SYSTEM_NAME,
            value: jackin_telemetry::Value::Str("jackin"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::RPC_METHOD,
            value: jackin_telemetry::Value::Str("jackin.host.Credentials/Resolve"),
        },
    ];
    if authenticate_caller(&stream, caller_auth).is_err() {
        complete_local_rpc_failure(&attrs);
        return Ok(());
    }

    // Read 4-byte BE length + JSON body (same framing as control channel).
    let request = async {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        anyhow::ensure!(len <= MAX_REQ, "request too large: {len}");
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body).await?;
        serde_json::from_slice::<CredRequest>(&body).context("parsing CredRequest")
    }
    .await;
    let Ok(req) = request else {
        complete_local_rpc_failure(&attrs);
        return Ok(());
    };
    let extracted = jackin_telemetry::propagation::extract(&req.ctx);
    if matches!(
        extracted,
        jackin_telemetry::propagation::ExtractOutcome::RejectRequest
    ) {
        let operation =
            jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs).ok();
        record_rpc_error(operation.as_ref());
        let write_result = stream
            .write_all(&frame(&CredReply::Error {
                error: "invalid correlation".to_owned(),
            }))
            .await;
        if let Some(operation) = operation {
            operation.complete(
                jackin_telemetry::schema::enums::OutcomeValue::Failure,
                Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
            );
        }
        drop(write_result);
        return Ok(());
    }
    let operation = match &extracted {
        jackin_telemetry::propagation::ExtractOutcome::Parent(parent) => {
            jackin_telemetry::operation_with_remote_parent(
                &jackin_telemetry::operation::RPC_SERVER,
                &attrs,
                parent,
            )
        }
        _ => jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, &attrs),
    }
    .ok();

    // Validate every requested ref against the operator-approved bindings.
    // Reject any ref that wasn't explicitly configured — this prevents a
    // compromised in-container process from escalating privileges by requesting
    // arbitrary op:// URIs or host env vars.
    let mut approved_refs = Vec::with_capacity(req.refs.len());
    for requested in &req.refs {
        let approved = allowed_bindings
            .iter()
            .find(|allowed| binding_matches_request(allowed, requested));
        let Some(approved) = approved else {
            record_rpc_error(operation.as_ref());
            let reply = CredReply::Error {
                error: "credential reference is not approved".to_owned(),
            };
            let write_result = stream.write_all(&frame(&reply)).await;
            if let Some(operation) = operation {
                operation.complete(
                    jackin_telemetry::schema::enums::OutcomeValue::Failure,
                    Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
                );
            }
            drop(write_result);
            return Ok(());
        };
        approved_refs.push(approved.clone());
    }

    let reply = if let Ok(values) = resolve(approved_refs).await {
        CredReply::Ok { values }
    } else {
        CredReply::Error {
            error: "credential resolution failed".to_owned(),
        }
    };
    // Reuse the canonical control-socket encoder so both ends of host.sock
    // frame identically.
    let succeeded = matches!(&reply, CredReply::Ok { .. });
    let write_result = stream.write_all(&frame(&reply)).await;
    if !succeeded || write_result.is_err() {
        record_rpc_error(operation.as_ref());
    }
    if let Some(operation) = operation {
        operation.complete(
            if succeeded && write_result.is_ok() {
                jackin_telemetry::schema::enums::OutcomeValue::Success
            } else {
                jackin_telemetry::schema::enums::OutcomeValue::Failure
            },
            (!succeeded || write_result.is_err())
                .then_some(jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
    drop(write_result);
    Ok(())
}

fn binding_matches_request(allowed: &ExecBinding, requested: &ExecBinding) -> bool {
    if allowed.name != requested.name || allowed.kind != requested.kind {
        return false;
    }
    match allowed.kind {
        ExecKind::Op | ExecKind::Env => allowed.source == requested.source,
        ExecKind::Literal => true,
    }
}

fn complete_local_rpc_failure(attrs: &[jackin_telemetry::Attr<'_>]) {
    let operation =
        jackin_telemetry::operation(&jackin_telemetry::operation::RPC_SERVER, attrs).ok();
    record_rpc_error(operation.as_ref());
    if let Some(operation) = operation {
        operation.complete(
            jackin_telemetry::schema::enums::OutcomeValue::Failure,
            Some(jackin_telemetry::schema::enums::ErrorType::RpcError),
        );
    }
}

fn record_rpc_error(operation: Option<&jackin_telemetry::OperationGuard>) {
    let record = || {
        let _error =
            jackin_telemetry::record_error(jackin_telemetry::schema::enums::ErrorType::RpcError);
    };
    if let Some(operation) = operation {
        operation.span().in_scope(record);
    } else {
        record();
    }
}

fn authenticate_caller(stream: &UnixStream, caller_auth: CallerAuth) -> Result<()> {
    match caller_auth {
        CallerAuth::CapsuleDaemon(expected) => authenticate_capsule_daemon_peer(stream, &expected),
        #[cfg(all(test, target_os = "linux"))]
        CallerAuth::PeerPid(expected) => {
            let actual = peer_pid(stream)?;
            anyhow::ensure!(
                actual == expected,
                "peer pid {actual} did not match expected pid {expected}"
            );
            Ok(())
        }
        #[cfg(all(test, not(target_os = "linux")))]
        CallerAuth::TestPeer => Ok(()),
    }
}

#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> Result<u32> {
    let cred = stream.peer_cred().context("reading peer credentials")?;
    let pid = cred
        .pid()
        .ok_or_else(|| anyhow::anyhow!("peer credentials did not include a pid"))?;
    u32::try_from(pid).context("peer pid was negative")
}

/// Captured only after runtime inspection of the launch-owned immutable ID.
#[derive(Clone, Debug)]
struct CapsulePeerIdentity {
    #[cfg(target_os = "linux")]
    process: std::sync::Arc<std::fs::File>,
    #[cfg(target_os = "linux")]
    namespace: std::sync::Arc<std::fs::File>,
    #[cfg(target_os = "linux")]
    pid: u32,
    #[cfg(target_os = "linux")]
    uid: u32,
    #[cfg(target_os = "linux")]
    start_time: u64,
    #[cfg(target_os = "linux")]
    container_id: String,
}

impl CapsulePeerIdentity {
    fn capture(container: &jackin_docker::docker_client::ContainerHandle, pid: u32) -> Result<Self> {
        anyhow::ensure!(!container.id().is_empty(), "immutable container ID is required");
        #[cfg(target_os = "linux")]
        {
            anyhow::ensure!(pid > 0, "runtime container init PID is unavailable");
            let process = std::sync::Arc::new(std::fs::File::open(format!("/proc/{pid}"))?);
            let path = pinned_process_path(&process);
            let cgroup = std::fs::read_to_string(path.join("cgroup"))?;
            anyhow::ensure!(process_belongs_to_container_cgroup(&cgroup, container.id()),
                "runtime init PID lacks local immutable container ownership proof");
            let status = std::fs::read_to_string(path.join("status"))?;
            anyhow::ensure!(peer_is_container_init_process_status(&status, pid), "runtime PID is not a direct container init");
            let uid = status.lines().find_map(|line| line.strip_prefix("Uid:"))
                .and_then(|uids| uids.split_whitespace().nth(1))
                .and_then(|uid| uid.parse().ok())
                .context("container init effective UID is unavailable")?;
            let start_time = process_start_time(&path)?;
            let namespace = std::sync::Arc::new(std::fs::File::open(path.join("ns/pid"))?);
            Ok(Self { process, namespace, pid, uid, start_time, container_id: container.id().to_owned() })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            ensure_caller_auth_supported()?;
            anyhow::bail!("container caller identity is unavailable")
        }
    }
}

#[cfg(target_os = "linux")]
fn pinned_process_path(process: &std::fs::File) -> PathBuf {
    use std::os::fd::AsRawFd;
    PathBuf::from(format!("/proc/self/fd/{}", process.as_raw_fd()))
}

#[cfg(target_os = "linux")]
fn process_start_time(path: &Path) -> Result<u64> {
    let stat = std::fs::read_to_string(path.join("stat"))?;
    stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().nth(19))
        .context("process start time missing")?.parse().context("invalid process start time")
}

#[cfg(target_os = "linux")]
fn authenticate_capsule_daemon_peer(stream: &UnixStream, expected: &CapsulePeerIdentity) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let cred = stream.peer_cred().context("reading peer credentials")?;
    anyhow::ensure!(peer_pid(stream)? == expected.pid && cred.uid() == expected.uid,
        "credential caller does not own this container relay");
    let path = pinned_process_path(&expected.process);
    anyhow::ensure!(process_start_time(&path)? == expected.start_time,
        "container init process identity changed");
    let namespace = std::fs::metadata(path.join("ns/pid"))?;
    let pinned = expected.namespace.metadata()?;
    anyhow::ensure!(namespace.dev() == pinned.dev() && namespace.ino() == pinned.ino(),
        "container PID namespace identity changed");
    anyhow::ensure!(process_belongs_to_container_cgroup(&std::fs::read_to_string(path.join("cgroup"))?, &expected.container_id),
        "caller lacks local immutable container ownership proof");
    anyhow::ensure!(peer_is_container_init_process_status(&std::fs::read_to_string(path.join("status"))?, expected.pid),
        "caller is not this container init");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn authenticate_capsule_daemon_peer(_stream: &UnixStream, _expected: &CapsulePeerIdentity) -> Result<()> {
    ensure_caller_auth_supported()
}

#[cfg(target_os = "linux")]
fn process_belongs_to_container_cgroup(cgroup: &str, container_id: &str) -> bool {
    // Full Docker IDs only. A remote daemon's host PID is meaningless locally;
    // its immutable ID must also own the local process's kernel cgroup path.
    if container_id.len() != 64 || !container_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return false;
    }
    let cgroupfs = format!("/docker/{container_id}");
    let systemd = format!("/system.slice/docker-{container_id}.scope");
    cgroup.lines().any(|line| {
        line.splitn(3, ':').nth(2).is_some_and(|path| path == cgroupfs || path == systemd)
    })
}

#[cfg(target_os = "linux")]
fn peer_is_container_init_process_status(status: &str, expected_pid: u32) -> bool {
    status
        .lines()
        .find_map(|line| line.strip_prefix("NSpid:"))
        .is_some_and(|value| {
            let mut ids = value.split_whitespace();
            let host_pid = ids.next().and_then(|value| value.parse::<u32>().ok());
            let container_pid = ids.next();
            host_pid == Some(expected_pid) && expected_pid > 0
                && container_pid == Some("1")
                && ids.next().is_none()
        })
}

async fn resolve_all(refs: &[ExecBinding]) -> Result<std::collections::BTreeMap<String, String>> {
    // Resolve concurrently: each `op` kind spawns an `op read` subprocess
    // (network + a possible Touch ID prompt, ~1-3s each), so serial resolution
    // would make interactive `jackin-exec` latency scale with the number of
    // selected credentials. Mirrors the parallel launch-time resolver.
    let resolved = futures_util::future::try_join_all(refs.iter().map(|r| async move {
        let value = resolve_one(r)
            .await
            .context("resolving approved credential")?;
        Ok::<_, anyhow::Error>((r.name.clone(), value))
    }))
    .await?;
    Ok(resolved.into_iter().collect())
}

fn validate_op_source(source: &str) -> Result<()> {
    anyhow::ensure!(
        source.starts_with("op://"),
        "invalid op:// reference {source:?}: must start with op://"
    );
    // Reject segments that look like CLI flags (start with -) to prevent arg injection.
    let path = &source["op://".len()..];
    anyhow::ensure!(
        !path.split('/').any(|s| s.starts_with('-')),
        "invalid op:// reference: segment looks like a flag in {source:?}"
    );
    Ok(())
}

async fn resolve_one(r: &ExecBinding) -> Result<String> {
    match r.kind {
        ExecKind::Op => {
            validate_op_source(&r.source).with_context(|| format!("credential {:?}", r.name))?;
            resolve_op(&r.source).await
        }
        ExecKind::Env => {
            // Reuse the canonical `$VAR` / `${VAR}` parser the binding collector
            // used to classify this source, so producer and consumer can't drift
            // on the host-ref grammar.
            let var_name = jackin_env::parse_host_ref(&r.source).ok_or_else(|| {
                anyhow::anyhow!(
                    "env credential {:?}: source {:?} is not a $VAR host reference",
                    r.name,
                    r.source
                )
            })?;
            std::env::var(var_name).with_context(|| format!("host env var {var_name:?} is not set"))
        }
        ExecKind::Literal => Ok(r.source.clone()),
    }
}

async fn resolve_op(op_ref: &str) -> Result<String> {
    // Insert -- end-of-options sentinel to prevent argument injection
    // via crafted op:// values containing flags. No timeout: Touch ID
    // prompts may block arbitrarily long (same semantic as pre-transport).
    let request = jackin_process::ExecRequest::new("op", ["read", "--", op_ref]).no_timeout();
    let output = crate::process_telemetry::exec_async(&request)
        .await
        .context("running credential process")?;

    if output.success {
        let raw = String::from_utf8_lossy(&output.stdout);
        Ok(raw.trim_end_matches('\n').to_owned())
    } else {
        anyhow::bail!("credential process exited unsuccessfully")
    }
}

#[cfg(test)]
mod tests;
