//! Host-side fetch of the in-container `jackin-capsule` daemon's
//! tab/pane snapshot.
//!
//! The daemon's socket is bind-mounted from
//! `paths.jackin_home/sockets/<container_name>/jackin.sock` so the
//! host can talk to it directly via a `UnixStream` — no `docker
//! exec`, no second client process.
//!
//! Schema sharing: the protocol types live in `jackin_protocol`, a
//! small shared crate. The host CLI and in-container Capsule both
//! depend on it, so request and reply structs are imported verbatim.
//! Drift between the two surfaces is a compile error, not a
//! wire-format bug.
//!
//! Sync API by design: the only caller today is
//! `ManagerState::refresh_instances`, which runs inside the host
//! TUI's render loop. A blocking std `UnixStream` round-trip (a few
//! bytes each way against an in-process daemon over the host
//! filesystem) is ~ms; the refresh path is already throttled to
//! 500 ms and we'd rather pay that few-ms latency than add a tokio
//! task + channel + lock + ordering guarantees just to keep the call
//! async.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use jackin_protocol::control::{ClientMsg, ServerMsg, TabSnapshot, frame as control_frame};

use crate::paths::JackinPaths;

/// Mirror of `socket::MAX_CONTROL_MSG` in the in-container crate.
const MAX_CONTROL_REPLY: usize = 4 * 1024 * 1024;

/// Per-call socket timeout. The whole round-trip is "open socket,
/// write 5 + json bytes, read 4 + json bytes" — anything beyond a
/// couple seconds means the daemon is wedged or the bind-mount
/// points at a stale dir. The console's preview pane re-polls on a
/// cadence, so a short timeout here keeps a dead container from
/// stalling the UI.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct InstanceSnapshot {
    pub tabs: Vec<TabSnapshot>,
    pub active_tab: u32,
}

/// Build the host-side path of a container's daemon socket. Matches
/// the bind-mount source set up in `runtime/launch.rs`.
pub fn socket_path(paths: &JackinPaths, container_name: &str) -> PathBuf {
    paths
        .jackin_home
        .join("sockets")
        .join(container_name)
        .join("jackin.sock")
}

/// Connect to the container's daemon socket and fetch its tab/pane
/// snapshot.
///
/// Returns `Ok(None)` when the socket file is absent (the container
/// is not running yet, was removed, or pre-dates this bind-mount
/// feature); returns `Err` only for genuine wire-level failures so
/// callers can log them.
pub fn fetch_snapshot(
    paths: &JackinPaths,
    container_name: &str,
) -> Result<Option<InstanceSnapshot>> {
    let path = socket_path(paths, container_name);
    if !path.exists() {
        return Ok(None);
    }
    fetch_snapshot_inner(&path).map(Some)
}

fn fetch_snapshot_inner(path: &Path) -> Result<InstanceSnapshot> {
    let mut stream = UnixStream::connect(path)
        .with_context(|| format!("connecting to daemon socket {}", path.display()))?;
    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .context("setting read timeout")?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .context("setting write timeout")?;

    stream
        .write_all(&control_frame(&ClientMsg::Snapshot))
        .context("writing Snapshot request to daemon")?;

    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .context("reading reply length")?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_CONTROL_REPLY {
        bail!("daemon reply length {len} exceeds limit {MAX_CONTROL_REPLY}");
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).context("reading reply body")?;

    let msg: ServerMsg = serde_json::from_slice(&body).context("parsing reply JSON")?;
    match msg {
        ServerMsg::Snapshot { tabs, active_tab } => Ok(InstanceSnapshot { tabs, active_tab }),
        ServerMsg::SessionList { .. } => {
            bail!("daemon replied with SessionList; expected Snapshot")
        }
    }
}
