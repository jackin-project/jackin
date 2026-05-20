/// Unix domain socket server.
///
/// Listens on `/run/jackin/jackin.sock`. Host CLI and future daemon
/// connect here to query session state or attach a client terminal.
///
/// Protocol: 4-byte big-endian length-prefixed JSON frames (see protocol.rs).

pub const SOCKET_PATH: &str = "/run/jackin/jackin.sock";

use std::path::Path;

use anyhow::Result;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc;

use crate::protocol::{ClientMsg, ServerMsg, b64_decode, b64_encode, frame};

/// A connected client handle.
pub struct Client {
    pub stream: UnixStream,
}

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

/// Read one framed message from a UnixStream. Returns None on EOF.
pub async fn read_msg(stream: &mut UnixStream) -> Option<ClientMsg> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.ok()?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 4 * 1024 * 1024 {
        return None;
    } // 4 MiB sanity guard
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body).await.ok()?;
    serde_json::from_slice(&body).ok()
}

/// Write one framed message to a UnixStream.
pub async fn write_msg(stream: &mut UnixStream, msg: &ServerMsg) -> Result<()> {
    let framed = frame(msg);
    stream.write_all(&framed).await?;
    Ok(())
}

/// Handle a one-shot status query from a non-attach client (e.g. host CLI).
/// Sends a `SessionList` response and closes the connection.
pub async fn handle_status_query(
    mut stream: UnixStream,
    sessions: Vec<crate::protocol::SessionInfo>,
) {
    let msg = ServerMsg::SessionList { sessions };
    let _ = write_msg(&mut stream, &msg).await;
}

/// Encode raw output bytes for transport in a ServerMsg::Output.
pub fn encode_output(data: &[u8]) -> ServerMsg {
    ServerMsg::Output {
        data: b64_encode(data),
    }
}

/// Decode input bytes from a ClientMsg::Input.
pub fn decode_input(data: &str) -> Vec<u8> {
    b64_decode(data)
}
