/// Unix domain socket server.
///
/// Listens on `/run/jackin/jackin.sock`. Two protocols share the socket:
/// the **control channel** (length-prefixed JSON, used by the host CLI
/// for one-shot queries) and the **attach channel** (binary tag+length
/// frames, used by interactive clients). The two are disambiguated by
/// the first byte of the connection — `0x00` means a length prefix
/// (control), anything else is an attach-channel tag.
pub const SOCKET_PATH: &str = "/run/jackin/jackin.sock";

use std::path::Path;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use crate::protocol::control::{ClientMsg, ServerMsg, SessionInfo, frame};

/// Start the Unix socket listener. Returns a receiver of newly-connected
/// clients. The caller (daemon) accepts clients from the channel.
pub fn start_listener() -> Result<mpsc::UnboundedReceiver<UnixStream>> {
    let path = Path::new(SOCKET_PATH);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(path)?;
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let _ = tx.send(stream);
                }
                Err(e) => {
                    eprintln!("[jackin-container] socket accept error: {e}");
                }
            }
        }
    });

    Ok(rx)
}

/// Read a length-prefixed JSON control message whose 4-byte length
/// prefix begins with `first_byte = 0x00` (already consumed by the
/// dispatcher).
pub async fn read_control_msg(stream: &mut UnixStream, first_byte: u8) -> Option<ClientMsg> {
    let mut rest = [0u8; 3];
    stream.read_exact(&mut rest).await.ok()?;
    let len_buf = [first_byte, rest[0], rest[1], rest[2]];
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 4 * 1024 * 1024 {
        return None;
    }
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await.ok()?;
    serde_json::from_slice(&body).ok()
}

/// Handle a one-shot control request and close the connection.
pub async fn handle_control_request(
    mut stream: UnixStream,
    first_byte: u8,
    sessions: Vec<SessionInfo>,
) {
    let Some(msg) = read_control_msg(&mut stream, first_byte).await else {
        return;
    };
    let reply = match msg {
        ClientMsg::Status => ServerMsg::SessionList { sessions },
    };
    let _ = stream.write_all(&frame(&reply)).await;
}
