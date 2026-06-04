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

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// A single on-demand credential binding resolved by the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecCredRef {
    /// The env var name that will be injected (e.g. "GH_TOKEN").
    pub name: String,
    /// Resolution kind: "op" → `op read <source>`, "env" → host env var,
    /// "literal" → return source verbatim.
    pub kind: String,
    /// The source to resolve: `op://` URI, `$VAR_NAME`, or literal string.
    pub source: String,
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

/// Start the host.sock listener. Returns a `JoinHandle` the caller can
/// cancel or await. The socket file is created at `sock_path`; the
/// caller is responsible for ensuring the parent directory is already
/// bind-mounted into the container.
///
/// The listener accepts one connection at a time (serialised) since
/// the capsule daemon sends exactly one request per `jackin-exec` call.
pub fn start(sock_path: PathBuf) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(e) = run_listener(&sock_path).await {
            crate::debug_log!("exec_host", "listener error: {e:#}");
        }
    })
}

async fn run_listener(sock_path: &Path) -> Result<()> {
    // Remove stale socket from a previous session.
    let _ = std::fs::remove_file(sock_path);
    if let Some(parent) = sock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(sock_path)
        .with_context(|| format!("binding host.sock at {}", sock_path.display()))?;

    crate::debug_log!(
        "exec_host",
        "listening at {}",
        sock_path.display()
    );

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                if let Err(e) = handle_connection(stream).await {
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

async fn handle_connection(mut stream: UnixStream) -> Result<()> {
    // Read 4-byte BE length + JSON body (same framing as control channel).
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    const MAX_REQ: usize = 512 * 1024;
    anyhow::ensure!(len <= MAX_REQ, "request too large: {len}");

    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await?;

    let req: CredRequest = serde_json::from_slice(&body).context("parsing CredRequest")?;

    crate::debug_log!(
        "exec_host",
        "resolving {} credential(s)",
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

async fn resolve_all(
    refs: &[ExecCredRef],
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut values = std::collections::BTreeMap::new();
    for r in refs {
        let value = resolve_one(r)
            .await
            .with_context(|| format!("resolving credential {:?}", r.name))?;
        values.insert(r.name.clone(), value);
    }
    Ok(values)
}

async fn resolve_one(r: &ExecCredRef) -> Result<String> {
    match r.kind.as_str() {
        "op" => resolve_op(&r.source).await,
        "env" => {
            let var_name = r.source.trim_start_matches('$').trim_start_matches('{').trim_end_matches('}');
            std::env::var(var_name)
                .with_context(|| format!("host env var {var_name:?} is not set"))
        }
        "literal" => Ok(r.source.clone()),
        other => anyhow::bail!("unknown credential kind {:?}", other),
    }
}

async fn resolve_op(op_ref: &str) -> Result<String> {
    let output = tokio::process::Command::new("op")
        .args(["read", op_ref])
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
