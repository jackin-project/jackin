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
    Hello { rows: u16, cols: u16 },
    Resize { rows: u16, cols: u16 },
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
        ClientFrame::Hello { rows, cols } => {
            let mut p = [0u8; 4];
            p[..2].copy_from_slice(&rows.to_be_bytes());
            p[2..].copy_from_slice(&cols.to_be_bytes());
            encode(TAG_HELLO, &p)
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

/// Read the next client frame from the stream. `first_byte` is the
/// already-peeked first byte (used by the channel-dispatch layer).
pub async fn read_client_frame(
    stream: &mut UnixStream,
    first_byte: u8,
) -> Result<Option<ClientFrame>> {
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
    Ok(Some(decode_client(first_byte, payload)?))
}

/// Read the next server frame from the stream. `first_byte` is the
/// already-read tag byte.
pub async fn read_server_frame(
    stream: &mut UnixStream,
    first_byte: u8,
) -> Result<Option<ServerFrame>> {
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
    Ok(Some(decode_server(first_byte, payload)?))
}

fn decode_client(tag: u8, payload: Vec<u8>) -> Result<ClientFrame> {
    Ok(match tag {
        TAG_HELLO => {
            if payload.len() < 4 {
                bail!("hello payload too short");
            }
            ClientFrame::Hello {
                rows: u16::from_be_bytes([payload[0], payload[1]]),
                cols: u16::from_be_bytes([payload[2], payload[3]]),
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
        });
        // First byte is tag, never `0x00` (which is reserved for the
        // control-channel JSON length high byte).
        assert_eq!(bytes[0], TAG_HELLO);
        assert_ne!(bytes[0], 0x00);
    }
}
