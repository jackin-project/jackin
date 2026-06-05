//! Host-side credential resolver for `jackin-exec`.
//!
//! Listens on a Unix socket at `~/.jackin/sockets/<container>/host.sock`
//! which is bind-mounted into the role container at `/jackin/run/host.sock`.
//! When the capsule daemon confirms an `ExecCommand` and the operator has
//! selected credentials in the picker, the capsule connects here to resolve
//! the on-demand env vars before running the command.
//!
//! This is a Phase 1 ad-hoc design — the `jackin load` process stays alive
//! for the session and this listener runs as a `tokio::spawn` task alongside
//! the blocking `docker exec -it` call. Future work: migrate to the jackin'
//! daemon so all running containers share one host-side resolver.
//!
//! # Security
//!
//! The listener validates every incoming resolution request against the
//! `allowed_bindings` set configured at session start. Only (name, kind,
//! source) triples that exactly match an operator-configured binding are
//! resolved. Unknown refs are rejected with a `CredError` without calling
//! `op` or reading any host env var. This prevents a compromised in-container
//! process from requesting arbitrary secret resolution via the host socket.
//!
//! For `kind = "op"`, `source` must start with `op://` and the `--`
//! end-of-options sentinel is inserted before passing to `op read` to prevent
//! argument injection via crafted op:// values.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// A single on-demand credential binding resolved by the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecCredRef {
    /// The env var name that will be injected (e.g. "`GH_TOKEN`").
    pub name: String,
    /// Resolution kind: "op" → `op read <source>`, "env" → host env var,
    /// "literal" → return source verbatim.
    pub kind: String,
    /// The source to resolve: `op://` URI, `$VAR_NAME`, or literal string.
    pub source: String,
}

impl From<&jackin_protocol::ExecBinding> for ExecCredRef {
    fn from(b: &jackin_protocol::ExecBinding) -> Self {
        Self {
            name: b.name.clone(),
            kind: b.kind.clone(),
            source: b.source.clone(),
        }
    }
}

/// Request from capsule → host.
#[derive(Debug, Deserialize)]
struct CredRequest {
    refs: Vec<ExecCredRef>,
}

/// Success response from host → capsule.
#[derive(Debug, Serialize)]
struct CredResponse {
    values: std::collections::BTreeMap<String, String>,
}

/// Error response from host → capsule.
#[derive(Debug, Serialize)]
struct CredError {
    error: String,
}

/// Start the host.sock listener.
///
/// Returns a `JoinHandle` the caller can cancel or await. The socket file is
/// created at `sock_path`; the caller is responsible for ensuring the parent
/// directory is already bind-mounted into the container.
///
/// `allowed_bindings` is the exhaustive set of credential refs the operator
/// configured for this session. Only refs in this set are resolved; any
/// incoming request that references an unknown (name, kind, source) triple
/// is rejected, preventing escalation from a compromised in-container process.
pub fn start(
    sock_path: PathBuf,
    allowed_bindings: Vec<ExecCredRef>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run_listener(&sock_path, &allowed_bindings).await {
            crate::debug_log!("exec_host", "listener error: {e:#}");
        }
    })
}

/// Start the host.sock listener for a named container.
///
/// Resolves the per-container socket path under
/// `<jackin_home>/sockets/<container>/host.sock` — the directory the launch
/// path bind-mounts to `/jackin/run` — maps the operator's `exec_bindings`
/// to the allowed-resolution set, and spawns the listener. Shared by both the
/// Docker and apple-container launch paths.
pub fn start_for_container(
    jackin_home: &Path,
    container_name: &str,
    exec_bindings: &[jackin_protocol::ExecBinding],
) -> tokio::task::JoinHandle<()> {
    let sock_path = jackin_home
        .join("sockets")
        .join(container_name)
        .join("host.sock");
    let allowed = exec_bindings.iter().map(ExecCredRef::from).collect();
    start(sock_path, allowed)
}

async fn run_listener(sock_path: &Path, allowed_bindings: &[ExecCredRef]) -> Result<()> {
    // Remove stale socket from a previous session.
    let _ = std::fs::remove_file(sock_path);
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
        // host.sock is the credential-resolution boundary: any process that can
        // connect and send an allow-listed (name,kind,source) triple gets the
        // secret resolved. Lock the directory to 0o700 so only the operator's
        // UID can reach the socket — independent of which backend created the
        // dir (the Docker launch path also sets this; the apple-container path
        // does not, so enforce it here at the shared listener choke point).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let listener = UnixListener::bind(sock_path)
        .with_context(|| format!("binding host.sock at {}", sock_path.display()))?;

    crate::debug_log!(
        "exec_host",
        "listening at {} with {} allowed bindings",
        sock_path.display(),
        allowed_bindings.len()
    );

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                if let Err(e) = handle_connection(stream, allowed_bindings).await {
                    crate::debug_log!("exec_host", "connection error: {e:#}");
                }
            }
            Err(e) => {
                crate::debug_log!("exec_host", "accept error: {e:#}");
                // Brief back-off to avoid tight loop on persistent errors.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
}

async fn handle_connection(mut stream: UnixStream, allowed_bindings: &[ExecCredRef]) -> Result<()> {
    const MAX_REQ: usize = 512 * 1024;
    // Read 4-byte BE length + JSON body (same framing as control channel).
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    anyhow::ensure!(len <= MAX_REQ, "request too large: {len}");

    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let req: CredRequest = serde_json::from_slice(&body).context("parsing CredRequest")?;

    // Validate every requested ref against the operator-approved bindings.
    // Reject any ref that wasn't explicitly configured — this prevents a
    // compromised in-container process from escalating privileges by requesting
    // arbitrary op:// URIs or host env vars.
    for r in &req.refs {
        let approved = allowed_bindings
            .iter()
            .any(|b| b.name == r.name && b.kind == r.kind && b.source == r.source);
        if !approved {
            crate::debug_log!(
                "exec_host",
                "rejected unauthorized ref: name={:?} kind={:?} source={:?}",
                r.name,
                r.kind,
                r.source
            );
            let err = CredError {
                error: format!(
                    "credential {:?} is not in the approved binding set for this session",
                    r.name
                ),
            };
            let reply_bytes = serde_json::to_vec(&err)?;
            let len_bytes = (reply_bytes.len() as u32).to_be_bytes();
            stream.write_all(&len_bytes).await?;
            stream.write_all(&reply_bytes).await?;
            return Ok(());
        }
    }

    crate::debug_log!(
        "exec_host",
        "resolving {} approved credential(s)",
        req.refs.len()
    );

    let reply_bytes = match resolve_all(&req.refs).await {
        Ok(values) => {
            let resp = CredResponse { values };
            serde_json::to_vec(&resp)?
        }
        Err(e) => {
            let err = CredError {
                error: format!("{e:#}"),
            };
            serde_json::to_vec(&err)?
        }
    };

    // Write 4-byte length + JSON body.
    let len_bytes = (reply_bytes.len() as u32).to_be_bytes();
    stream.write_all(&len_bytes).await?;
    stream.write_all(&reply_bytes).await?;
    Ok(())
}

async fn resolve_all(refs: &[ExecCredRef]) -> Result<std::collections::BTreeMap<String, String>> {
    let mut values = std::collections::BTreeMap::new();
    for r in refs {
        let value = resolve_one(r)
            .await
            .with_context(|| format!("resolving credential {:?}", r.name))?;
        values.insert(r.name.clone(), value);
    }
    Ok(values)
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

async fn resolve_one(r: &ExecCredRef) -> Result<String> {
    match r.kind.as_str() {
        "op" => {
            validate_op_source(&r.source).with_context(|| format!("credential {:?}", r.name))?;
            resolve_op(&r.source).await
        }
        "env" => {
            // Strip leading `$`, optional `{`, and trailing `}` to extract the var name.
            let src = r.source.as_str();
            let var_name = src
                .strip_prefix('$')
                .map_or(src, |s| s.trim_matches('{').trim_matches('}'));
            std::env::var(var_name).with_context(|| format!("host env var {var_name:?} is not set"))
        }
        "literal" => Ok(r.source.clone()),
        other => anyhow::bail!("unknown credential kind {other:?}"),
    }
}

async fn resolve_op(op_ref: &str) -> Result<String> {
    let output = tokio::process::Command::new("op")
        // Insert -- end-of-options sentinel to prevent argument injection
        // via crafted op:// values containing flags.
        .args(["read", "--", op_ref])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning `op read`")?
        .wait_with_output()
        .await
        .context("waiting for `op read`")?;

    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout);
        Ok(raw.trim_end_matches('\n').to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("`op read` failed: {}", stderr.trim())
    }
}

#[cfg(test)]
mod tests;
