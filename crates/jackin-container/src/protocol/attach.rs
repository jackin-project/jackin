/// Attach channel: tag-plus-length binary framing.
///
/// Used by interactive clients: one persistent connection per attached
/// terminal. Each frame is `[1-byte tag][4-byte BE length][payload]`.
/// Payload of `InputBytes` / `OutputBytes` is raw PTY bytes — no base64,
/// no JSON nesting on the hot path.
///
/// Disambiguation from the control channel: every binary tag is in
/// `0x01..=0xFF`. The control channel's first byte is the top byte of a
/// 4-byte big-endian length, which is always `0x00` for the message
/// sizes the host CLI sends. Reading the first byte tells the daemon
/// which protocol the client is speaking.
use anyhow::{Result, bail};
use tokio::io::AsyncReadExt;
use tokio::net::UnixStream;

// Client → server tags.
pub const TAG_HELLO: u8 = 0x01;
pub const TAG_RESIZE: u8 = 0x02;
pub const TAG_INPUT: u8 = 0x03;
pub const TAG_COMMAND: u8 = 0x04;
pub const TAG_DETACH: u8 = 0x05;
pub const TAG_FOCUS_IN: u8 = 0x06;
pub const TAG_FOCUS_OUT: u8 = 0x07;

// Server → client tags. The top bit is set as a convention so a future
// reader can tell direction by glancing at the byte.
pub const TAG_WELCOME: u8 = 0x81;
pub const TAG_OUTPUT: u8 = 0x82;
pub const TAG_SESSION_LIST: u8 = 0x83;
pub const TAG_SHUTDOWN: u8 = 0x84;
pub const TAG_BELL: u8 = 0x85;

const MAX_FRAME_PAYLOAD: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFrame {
    /// First frame from a newly-connected client. `spawn_agent` carries
    /// the agent slug the host CLI requested via
    /// `docker exec ... jackin-container new <agent>` — the daemon
    /// spawns a fresh session for that agent before attach completes.
    /// Plain attach (operator-initiated reattach) sets `spawn_agent`
    /// to None.
    Hello {
        rows: u16,
        cols: u16,
        spawn_agent: Option<String>,
    },
    Resize {
        rows: u16,
        cols: u16,
    },
    Input(Vec<u8>),
    Command(Vec<u8>),
    Detach,
    FocusIn,
    FocusOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerFrame {
    Welcome { session_count: u32 },
    Output(Vec<u8>),
    SessionList(Vec<u8>),
    Shutdown,
    Bell,
}

/// Encode a single attach frame: `[tag][length BE u32][payload]`.
pub fn encode(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(tag);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Encode a server frame.
pub fn encode_server(frame: ServerFrame) -> Vec<u8> {
    match frame {
        ServerFrame::Welcome { session_count } => encode(TAG_WELCOME, &session_count.to_be_bytes()),
        ServerFrame::Output(bytes) => encode(TAG_OUTPUT, &bytes),
        ServerFrame::SessionList(json) => encode(TAG_SESSION_LIST, &json),
        ServerFrame::Shutdown => encode(TAG_SHUTDOWN, &[]),
        ServerFrame::Bell => encode(TAG_BELL, &[]),
    }
}

/// Encode a client frame.
pub fn encode_client(frame: ClientFrame) -> Vec<u8> {
    match frame {
        ClientFrame::Hello {
            rows,
            cols,
            spawn_agent,
        } => {
            // Layout: rows(2) cols(2) agent_len(2) agent_bytes(N).
            // agent_len == 0 means "no spawn intent" so an older
            // 4-byte Hello stays parseable as a 6-byte Hello with
            // zero-length agent (forward-compatible).
            let agent_bytes = spawn_agent.as_deref().unwrap_or("").as_bytes();
            let agent_len = u16::try_from(agent_bytes.len()).unwrap_or(0);
            let mut payload = Vec::with_capacity(6 + agent_bytes.len());
            payload.extend_from_slice(&rows.to_be_bytes());
            payload.extend_from_slice(&cols.to_be_bytes());
            payload.extend_from_slice(&agent_len.to_be_bytes());
            payload.extend_from_slice(&agent_bytes[..agent_len as usize]);
            encode(TAG_HELLO, &payload)
        }
        ClientFrame::Resize { rows, cols } => {
            let mut p = [0u8; 4];
            p[..2].copy_from_slice(&rows.to_be_bytes());
            p[2..].copy_from_slice(&cols.to_be_bytes());
            encode(TAG_RESIZE, &p)
        }
        ClientFrame::Input(bytes) => encode(TAG_INPUT, &bytes),
        ClientFrame::Command(json) => encode(TAG_COMMAND, &json),
        ClientFrame::Detach => encode(TAG_DETACH, &[]),
        ClientFrame::FocusIn => encode(TAG_FOCUS_IN, &[]),
        ClientFrame::FocusOut => encode(TAG_FOCUS_OUT, &[]),
    }
}

/// Read one length-prefixed payload from `stream` given the already-
/// peeked first byte (the frame's tag). Returns `Ok(None)` on EOF /
/// disconnect, `Err` on oversized length. Used by both
/// `read_client_frame` and `read_server_frame` — keeping the framing
/// in one place means a future tightening of `MAX_FRAME_PAYLOAD` (or
/// a switch to streaming) only has to touch this function.
async fn read_framed_payload(
    stream: &mut UnixStream,
    first_byte: u8,
) -> Result<Option<(u8, Vec<u8>)>> {
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).await.is_err() {
        return Ok(None);
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_PAYLOAD {
        bail!("attach frame payload {len} exceeds limit {MAX_FRAME_PAYLOAD}");
    }
    let mut payload = vec![0u8; len];
    if !payload.is_empty() && stream.read_exact(&mut payload).await.is_err() {
        return Ok(None);
    }
    Ok(Some((first_byte, payload)))
}

/// Read the next client frame from the stream. `first_byte` is the
/// already-peeked first byte (used by the channel-dispatch layer).
pub async fn read_client_frame(
    stream: &mut UnixStream,
    first_byte: u8,
) -> Result<Option<ClientFrame>> {
    let Some((tag, payload)) = read_framed_payload(stream, first_byte).await? else {
        return Ok(None);
    };
    Ok(Some(decode_client(tag, payload)?))
}

/// Read the next server frame from the stream. `first_byte` is the
/// already-read tag byte.
pub async fn read_server_frame(
    stream: &mut UnixStream,
    first_byte: u8,
) -> Result<Option<ServerFrame>> {
    let Some((tag, payload)) = read_framed_payload(stream, first_byte).await? else {
        return Ok(None);
    };
    Ok(Some(decode_server(tag, payload)?))
}

fn decode_client(tag: u8, payload: Vec<u8>) -> Result<ClientFrame> {
    Ok(match tag {
        TAG_HELLO => {
            if payload.len() < 4 {
                bail!("hello payload too short");
            }
            let rows = u16::from_be_bytes([payload[0], payload[1]]);
            let cols = u16::from_be_bytes([payload[2], payload[3]]);
            let spawn_agent = if payload.len() >= 6 {
                let agent_len = u16::from_be_bytes([payload[4], payload[5]]) as usize;
                if payload.len() < 6 + agent_len {
                    bail!(
                        "hello agent_len {agent_len} exceeds payload size {}",
                        payload.len()
                    );
                }
                if agent_len == 0 {
                    None
                } else {
                    let slug = std::str::from_utf8(&payload[6..6 + agent_len])
                        .map_err(|_| anyhow::anyhow!("hello agent slug is not valid UTF-8"))?;
                    Some(slug.to_string())
                }
            } else {
                None
            };
            ClientFrame::Hello {
                rows,
                cols,
                spawn_agent,
            }
        }
        TAG_RESIZE => {
            if payload.len() < 4 {
                bail!("resize payload too short");
            }
            ClientFrame::Resize {
                rows: u16::from_be_bytes([payload[0], payload[1]]),
                cols: u16::from_be_bytes([payload[2], payload[3]]),
            }
        }
        TAG_INPUT => ClientFrame::Input(payload),
        TAG_COMMAND => ClientFrame::Command(payload),
        TAG_DETACH => ClientFrame::Detach,
        TAG_FOCUS_IN => ClientFrame::FocusIn,
        TAG_FOCUS_OUT => ClientFrame::FocusOut,
        other => bail!("unknown client attach tag {other:#04x}"),
    })
}

fn decode_server(tag: u8, payload: Vec<u8>) -> Result<ServerFrame> {
    Ok(match tag {
        TAG_WELCOME => {
            if payload.len() < 4 {
                bail!("welcome payload too short");
            }
            ServerFrame::Welcome {
                session_count: u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]),
            }
        }
        TAG_OUTPUT => ServerFrame::Output(payload),
        TAG_SESSION_LIST => ServerFrame::SessionList(payload),
        TAG_SHUTDOWN => ServerFrame::Shutdown,
        TAG_BELL => ServerFrame::Bell,
        other => bail!("unknown server attach tag {other:#04x}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_path_output_avoids_base64_and_json() {
        // Regression for the first attempt's `base64-inside-JSON` hot path:
        // a 4 KiB chunk of raw PTY bytes must travel through the attach
        // channel with only 5 bytes of overhead (tag + length).
        let payload = vec![0xCDu8; 4096];
        let frame = encode_server(ServerFrame::Output(payload.clone()));
        assert_eq!(frame.len(), 5 + payload.len());
        assert_eq!(frame[0], TAG_OUTPUT);
        assert_eq!(&frame[1..5], &(payload.len() as u32).to_be_bytes());
        assert_eq!(&frame[5..], &payload[..]);
    }

    #[test]
    fn hello_roundtrips() {
        let bytes = encode_client(ClientFrame::Hello {
            rows: 42,
            cols: 100,
            spawn_agent: None,
        });
        // First byte is tag, never `0x00` (which is reserved for the
        // control-channel JSON length high byte).
        assert_eq!(bytes[0], TAG_HELLO);
        assert_ne!(bytes[0], 0x00);
    }

    #[test]
    fn hello_with_spawn_agent_roundtrips() {
        let bytes = encode_client(ClientFrame::Hello {
            rows: 50,
            cols: 200,
            spawn_agent: Some("codex".to_string()),
        });
        // Decode skips the 4-byte length prefix that `encode_client` writes
        // after the tag; reconstruct the payload to feed `decode_client`.
        let payload = bytes[5..].to_vec();
        let frame = decode_client(TAG_HELLO, payload).unwrap();
        assert_eq!(
            frame,
            ClientFrame::Hello {
                rows: 50,
                cols: 200,
                spawn_agent: Some("codex".to_string()),
            }
        );
    }
}
