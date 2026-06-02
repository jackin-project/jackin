//! Attach protocol handshake: initial capability negotiation and session-ID
//! assignment when a client connects.
//!
//! Not responsible for: PTY lifecycle, input dispatch after attach completes,
//! or control-channel framing (see `protocol::control`).
//!
//! Key invariant: every client → server tag is in `0x01..=0x7F`; every
//! server → client tag has the top bit set (`0x80..=0xFF`). The first byte
//! of a new connection lands in the daemon's protocol-disambiguator before
//! this module sees it.

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
use std::time::Duration;

use anyhow::{Context, Result, bail};
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
pub const MAX_HELLO_ENV: usize = 64;
/// Per-entry cap on Hello env-value byte length. Operator-supplied env
/// values in jackin' are short (slugs, booleans, file paths); cap at
/// 8 KiB so a buggy or hostile client cannot smuggle a megabyte-sized
/// env entry past MAX_HELLO_ENV (the count cap) into the spawned
/// session's environment block.
pub const MAX_HELLO_ENV_VALUE: usize = 8 * 1024;
/// Per-entry cap on Hello env-key byte length. Same shape as
/// MAX_HELLO_ENV_VALUE; env-var names should be even shorter than
/// values, but the wire field is still u16-sized so we bound it.
pub const MAX_HELLO_ENV_KEY: usize = 1024;
/// Per terminal-identity field cap. These values come from the active
/// attach client's environment (`TERM`, `TERM_PROGRAM`, `COLORTERM`) and
/// should be tiny, but bounding them keeps the Hello frame shape explicit.
pub const MAX_CLIENT_TERMINAL_FIELD: usize = 1024;

const TERM_LABEL: &str = "TERM";
const TERM_PROGRAM_LABEL: &str = "TERM_PROGRAM";
const COLORTERM_LABEL: &str = "COLORTERM";

/// Wall-clock cap for a single attach frame's read. Bounded so a peer
/// that stalls between length prefix and payload — or trickles bytes
/// slower than the bandwidth bound — cannot pin the per-connection
/// task. 4 MiB across 10 s is ~409 KiB/s, well below any reasonable
/// localhost rate.
const FRAME_READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnRequest {
    Shell,
    Agent(String),
    /// Agent spawn where the provider was already selected by the console
    /// before `docker exec`-ing. The daemon uses `provider_label` directly
    /// as the tab suffix instead of showing the in-mux ProviderPicker dialog.
    AgentWithProvider {
        slug: String,
        provider_label: String,
    },
}

impl SpawnRequest {
    /// Build an `Agent` variant rejecting empty slugs. Mirrors the
    /// decode-side `decode_client` check so in-process callers cannot
    /// construct a degenerate `Agent("")` that would only be caught
    /// after a wire round-trip.
    pub fn agent(slug: impl Into<String>) -> anyhow::Result<Self> {
        let slug = slug.into();
        if slug.is_empty() {
            anyhow::bail!("SpawnRequest::Agent slug must be non-empty");
        }
        Ok(SpawnRequest::Agent(slug))
    }
}

/// Wire cap for the provider label field in an `AgentWithProvider` Hello frame.
pub const MAX_HELLO_PROVIDER_LABEL: usize = 64;

/// Terminal identity reported by the currently attached client.
/// Per-attach, not container-lifetime: the daemon must gate
/// outer-terminal enhancements on this value rather than on the
/// daemon process environment inherited at launch, since a running
/// container can be reattached from a different terminal later.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientTerminal {
    pub term: Option<String>,
    pub term_program: Option<String>,
    pub colorterm: Option<String>,
}

impl ClientTerminal {
    pub fn from_env() -> Self {
        Self {
            term: non_empty_env(TERM_LABEL),
            term_program: non_empty_env(TERM_PROGRAM_LABEL),
            colorterm: non_empty_env(COLORTERM_LABEL),
        }
    }

    #[must_use]
    pub fn pointer_shapes_supported(&self) -> bool {
        let term = self.term.as_deref().unwrap_or("").to_ascii_lowercase();
        let term_program = self
            .term_program
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();

        if (term.is_empty() && term_program.is_empty()) || matches!(term.as_str(), "dumb" | "linux")
        {
            return false;
        }
        if term_program.contains("warp") {
            return false;
        }

        !term_program.is_empty()
            || term.contains("ghostty")
            || term.contains("kitty")
            || term.contains("foot")
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFrame {
    /// First frame from a newly-connected client. Plain attach sets
    /// `spawn` to None; `jackin-capsule new` uses `Shell` or
    /// `Agent(slug)` so the daemon can create the requested session
    /// before attach completes. `env` carries per-session overrides
    /// that the short-lived `docker exec` client must forward to the
    /// long-lived daemon.
    Hello {
        rows: u16,
        cols: u16,
        spawn: Option<SpawnRequest>,
        env: Vec<(String, String)>,
        /// Optional pane-focus request: when `Some(session_id)` the
        /// daemon switches its active tab + that tab's `focused_id`
        /// to the leaf carrying this session id before forwarding any
        /// content to the attached client. The host console emits
        /// this when the operator picks a specific pane out of the
        /// preview-pane snapshot so the operator lands inside the
        /// pane they selected. Unknown / missing session ids are
        /// ignored — the daemon attaches at the current focus and
        /// the operator can navigate.
        focus_session: Option<u64>,
        terminal: ClientTerminal,
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

/// Encode a client frame. Returns `Err` for inputs that overflow the
/// wire field widths (env count, agent slug, env key, env value).
/// Symmetric with `decode_client` so a producer learns about over-cap
/// input the same way a peer would, without panicking.
pub fn encode_client(frame: ClientFrame) -> Result<Vec<u8>> {
    Ok(match frame {
        ClientFrame::Hello {
            rows,
            cols,
            spawn,
            env,
            focus_session,
            terminal,
        } => {
            // Layout:
            //   rows(2) cols(2) spawn_kind(1)
            //   agent_len(2) agent_bytes(N)
            //   [kind=3 only] provider_label_len(2) provider_label_bytes(M)
            //   env_count(2)
            //   repeated key_len(2) value_len(4) key_bytes value_bytes
            //   focus_kind(1) [focus_session_id(8) if focus_kind == 1]
            //   term_len(2) term_bytes
            //   term_program_len(2) term_program_bytes
            //   colorterm_len(2) colorterm_bytes
            let (spawn_kind, agent_bytes, provider_label_bytes): (u8, &[u8], &[u8]) =
                match spawn.as_ref() {
                    None => (0, b"", b""),
                    Some(SpawnRequest::Shell) => (1, b"", b""),
                    Some(SpawnRequest::Agent(agent)) => (2, agent.as_bytes(), b""),
                    Some(SpawnRequest::AgentWithProvider {
                        slug,
                        provider_label,
                    }) => (3, slug.as_bytes(), provider_label.as_bytes()),
                };
            if env.len() > MAX_HELLO_ENV {
                bail!(
                    "hello env count {} exceeds wire cap {MAX_HELLO_ENV}",
                    env.len()
                );
            }
            let agent_len = u16::try_from(agent_bytes.len())
                .map_err(|_| anyhow::anyhow!("agent slug exceeds u16::MAX bytes on the wire"))?;
            let env_count = u16::try_from(env.len()).map_err(|_| {
                anyhow::anyhow!("hello env count exceeds u16::MAX entries on the wire")
            })?;
            let mut payload = Vec::with_capacity(10 + agent_bytes.len());
            payload.extend_from_slice(&rows.to_be_bytes());
            payload.extend_from_slice(&cols.to_be_bytes());
            payload.push(spawn_kind);
            payload.extend_from_slice(&agent_len.to_be_bytes());
            payload.extend_from_slice(agent_bytes);
            if spawn_kind == 3 {
                if provider_label_bytes.len() > MAX_HELLO_PROVIDER_LABEL {
                    bail!(
                        "provider label length {} exceeds cap {MAX_HELLO_PROVIDER_LABEL}",
                        provider_label_bytes.len()
                    );
                }
                let pl_len = u16::try_from(provider_label_bytes.len()).map_err(|_| {
                    anyhow::anyhow!("provider label exceeds u16::MAX bytes on the wire")
                })?;
                payload.extend_from_slice(&pl_len.to_be_bytes());
                payload.extend_from_slice(provider_label_bytes);
            }
            payload.extend_from_slice(&env_count.to_be_bytes());
            for (key, value) in env {
                let key_bytes = key.as_bytes();
                let value_bytes = value.as_bytes();
                if key_bytes.len() > MAX_HELLO_ENV_KEY {
                    bail!(
                        "hello env key {key:?} length {} exceeds cap {MAX_HELLO_ENV_KEY}",
                        key_bytes.len()
                    );
                }
                if value_bytes.len() > MAX_HELLO_ENV_VALUE {
                    bail!(
                        "hello env value for {key:?} length {} exceeds cap {MAX_HELLO_ENV_VALUE}",
                        value_bytes.len()
                    );
                }
                let key_len = u16::try_from(key_bytes.len()).map_err(|_| {
                    anyhow::anyhow!("hello env key {key:?} exceeds u16::MAX bytes on the wire")
                })?;
                let value_len = u32::try_from(value_bytes.len()).map_err(|_| {
                    anyhow::anyhow!(
                        "hello env value for {key:?} exceeds u32::MAX bytes on the wire"
                    )
                })?;
                payload.extend_from_slice(&key_len.to_be_bytes());
                payload.extend_from_slice(&value_len.to_be_bytes());
                payload.extend_from_slice(key_bytes);
                payload.extend_from_slice(value_bytes);
            }
            match focus_session {
                None => payload.push(0u8),
                Some(id) => {
                    payload.push(1u8);
                    payload.extend_from_slice(&id.to_be_bytes());
                }
            }
            write_terminal_field(&mut payload, terminal.term.as_deref(), TERM_LABEL)?;
            write_terminal_field(
                &mut payload,
                terminal.term_program.as_deref(),
                TERM_PROGRAM_LABEL,
            )?;
            write_terminal_field(&mut payload, terminal.colorterm.as_deref(), COLORTERM_LABEL)?;
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
    })
}

fn write_terminal_field(payload: &mut Vec<u8>, value: Option<&str>, label: &str) -> Result<()> {
    let bytes = value.unwrap_or("").as_bytes();
    if bytes.len() > MAX_CLIENT_TERMINAL_FIELD {
        bail!(
            "hello terminal field {label} length {} exceeds cap {MAX_CLIENT_TERMINAL_FIELD}",
            bytes.len()
        );
    }
    let len = u16::try_from(bytes.len())
        .map_err(|_| anyhow::anyhow!("hello terminal field {label} exceeds u16::MAX bytes"))?;
    payload.extend_from_slice(&len.to_be_bytes());
    payload.extend_from_slice(bytes);
    Ok(())
}

fn read_terminal_field(cursor: &mut PayloadCursor<'_>, label: &str) -> Result<Option<String>> {
    let len_label = format!("terminal {label} length");
    let len = cursor.read_u16(&len_label)? as usize;
    if len > MAX_CLIENT_TERMINAL_FIELD {
        bail!("hello terminal {label} length {len} exceeds cap {MAX_CLIENT_TERMINAL_FIELD}");
    }
    let value_label = format!("terminal {label}");
    let value = cursor.read_string(len, &value_label)?;
    Ok((!value.is_empty()).then_some(value))
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
    match tokio::time::timeout(FRAME_READ_TIMEOUT, stream.read_exact(&mut len_buf)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            // Clean EOF (peer closed before sending any length byte) is
            // the expected end-of-stream signal. Anything else — connection
            // reset, EPIPE, timeout — gets bubbled so the daemon clog
            // attributes the cause instead of silently treating it as EOF.
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(e).context("attach frame: reading length prefix");
        }
        Err(_) => bail!("attach frame: timed out reading length prefix"),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_PAYLOAD {
        bail!("attach frame payload {len} exceeds limit {MAX_FRAME_PAYLOAD}");
    }
    // Chunked-grow read avoids the eager `vec![0u8; len]` memset that
    // would zero-touch up to 4 MiB on every frame regardless of whether
    // the peer ever delivers the bytes. A stalled or trickle attacker
    // would otherwise burn gigabytes of memset bandwidth per
    // connection-second.
    let mut payload: Vec<u8> = Vec::with_capacity(len.min(64 * 1024));
    let mut remaining = len;
    let mut chunk = [0u8; 16 * 1024];
    while remaining > 0 {
        let n = chunk.len().min(remaining);
        match tokio::time::timeout(FRAME_READ_TIMEOUT, stream.read_exact(&mut chunk[..n])).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
                return Err(e).context("attach frame: reading payload");
            }
            Err(_) => bail!("attach frame: timed out reading payload"),
        }
        payload.extend_from_slice(&chunk[..n]);
        remaining -= n;
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

pub fn decode_client(tag: u8, payload: Vec<u8>) -> Result<ClientFrame> {
    Ok(match tag {
        TAG_HELLO => {
            if payload.len() < 4 {
                bail!("hello payload too short");
            }
            let mut cursor = PayloadCursor::new(&payload);
            let rows = cursor.read_u16("rows")?;
            let cols = cursor.read_u16("cols")?;
            let spawn_kind = cursor.read_u8("spawn kind")?;
            let agent_len = cursor.read_u16("agent length")? as usize;
            let agent = cursor.read_string(agent_len, "agent slug")?;
            let spawn = match spawn_kind {
                0 => None,
                1 => {
                    if !agent.is_empty() {
                        bail!("hello shell spawn must not carry an agent slug");
                    }
                    Some(SpawnRequest::Shell)
                }
                2 => {
                    if agent.is_empty() {
                        bail!("hello agent spawn missing slug");
                    }
                    Some(SpawnRequest::Agent(agent))
                }
                3 => {
                    if agent.is_empty() {
                        bail!("hello agent-with-provider spawn missing slug");
                    }
                    let pl_len = cursor.read_u16("provider label length")? as usize;
                    if pl_len > MAX_HELLO_PROVIDER_LABEL {
                        bail!(
                            "hello provider label length {pl_len} exceeds cap {MAX_HELLO_PROVIDER_LABEL}"
                        );
                    }
                    let provider_label = cursor.read_string(pl_len, "provider label")?;
                    if provider_label.is_empty() {
                        bail!("hello agent-with-provider spawn missing provider label");
                    }
                    Some(SpawnRequest::AgentWithProvider {
                        slug: agent,
                        provider_label,
                    })
                }
                other => bail!("unknown hello spawn kind {other}"),
            };
            let env_count = cursor.read_u16("env count")? as usize;
            if env_count > MAX_HELLO_ENV {
                bail!("hello env_count {env_count} exceeds limit {MAX_HELLO_ENV}");
            }
            let mut env = Vec::with_capacity(env_count);
            for _ in 0..env_count {
                let key_len = cursor.read_u16("env key length")? as usize;
                let value_len = cursor.read_u32("env value length")? as usize;
                if key_len > MAX_HELLO_ENV_KEY {
                    bail!("hello env key length {key_len} exceeds cap {MAX_HELLO_ENV_KEY}");
                }
                if value_len > MAX_HELLO_ENV_VALUE {
                    bail!("hello env value length {value_len} exceeds cap {MAX_HELLO_ENV_VALUE}");
                }
                let key = cursor.read_string(key_len, "env key")?;
                let value = cursor.read_string(value_len, "env value")?;
                env.push((key, value));
            }
            let focus_kind = cursor.read_u8("focus kind")?;
            let focus_session = match focus_kind {
                0 => None,
                1 => Some(cursor.read_u64("focus session id")?),
                other => bail!("unknown hello focus kind {other}"),
            };
            let terminal = ClientTerminal {
                term: read_terminal_field(&mut cursor, TERM_LABEL)?,
                term_program: read_terminal_field(&mut cursor, TERM_PROGRAM_LABEL)?,
                colorterm: read_terminal_field(&mut cursor, COLORTERM_LABEL)?,
            };
            if !cursor.finished() {
                bail!("hello payload has trailing bytes");
            }
            ClientFrame::Hello {
                rows,
                cols,
                spawn,
                env,
                focus_session,
                terminal,
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

pub fn decode_server(tag: u8, payload: Vec<u8>) -> Result<ServerFrame> {
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

struct PayloadCursor<'a> {
    payload: &'a [u8],
    pos: usize,
}

impl<'a> PayloadCursor<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, pos: 0 }
    }

    fn finished(&self) -> bool {
        self.pos == self.payload.len()
    }

    fn read_u8(&mut self, field: &str) -> Result<u8> {
        if self.pos >= self.payload.len() {
            bail!("hello payload ended before {field}");
        }
        let value = self.payload[self.pos];
        self.pos += 1;
        Ok(value)
    }

    fn read_u16(&mut self, field: &str) -> Result<u16> {
        let bytes = self.read_bytes(2, field)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self, field: &str) -> Result<u32> {
        let bytes = self.read_bytes(4, field)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_u64(&mut self, field: &str) -> Result<u64> {
        let bytes = self.read_bytes(8, field)?;
        let arr: [u8; 8] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("hello {field}: short slice"))?;
        Ok(u64::from_be_bytes(arr))
    }

    fn read_string(&mut self, len: usize, field: &str) -> Result<String> {
        let bytes = self.read_bytes(len, field)?;
        let s = std::str::from_utf8(bytes)
            .map_err(|_| anyhow::anyhow!("hello {field} is not valid UTF-8"))?;
        Ok(s.to_string())
    }

    fn read_bytes(&mut self, len: usize, field: &str) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!("hello {field} length overflow"))?;
        if end > self.payload.len() {
            bail!(
                "hello {field} length {len} exceeds remaining payload {}",
                self.payload.len().saturating_sub(self.pos)
            );
        }
        let bytes = &self.payload[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }
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
            spawn: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .unwrap();
        // First byte is tag, never `0x00` (which is reserved for the
        // control-channel JSON length high byte).
        assert_eq!(bytes[0], TAG_HELLO);
        assert_ne!(bytes[0], 0x00);
    }

    #[test]
    fn hello_with_spawn_shell_roundtrips() {
        let bytes = encode_client(ClientFrame::Hello {
            rows: 50,
            cols: 200,
            spawn: Some(SpawnRequest::Shell),
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .unwrap();
        let payload = bytes[5..].to_vec();
        let frame = decode_client(TAG_HELLO, payload).unwrap();
        assert_eq!(
            frame,
            ClientFrame::Hello {
                rows: 50,
                cols: 200,
                spawn: Some(SpawnRequest::Shell),
                env: Vec::new(),
                terminal: ClientTerminal::default(),
                focus_session: None,
            }
        );
    }

    #[test]
    fn hello_with_spawn_agent_and_env_roundtrips() {
        let bytes = encode_client(ClientFrame::Hello {
            rows: 50,
            cols: 200,
            spawn: Some(SpawnRequest::Agent("codex".to_string())),
            env: vec![
                ("JACKIN_GIT_COAUTHOR_TRAILER".to_string(), "1".to_string()),
                ("JACKIN_GIT_DCO".to_string(), "1".to_string()),
            ],
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .unwrap();
        // Decode skips the 4-byte length prefix that `encode_client` writes
        // after the tag; reconstruct the payload to feed `decode_client`.
        let payload = bytes[5..].to_vec();
        let frame = decode_client(TAG_HELLO, payload).unwrap();
        assert_eq!(
            frame,
            ClientFrame::Hello {
                rows: 50,
                cols: 200,
                spawn: Some(SpawnRequest::Agent("codex".to_string())),
                env: vec![
                    ("JACKIN_GIT_COAUTHOR_TRAILER".to_string(), "1".to_string()),
                    ("JACKIN_GIT_DCO".to_string(), "1".to_string()),
                ],
                terminal: ClientTerminal::default(),
                focus_session: None,
            }
        );
    }

    #[test]
    fn hello_with_agent_and_provider_roundtrips() {
        // spawn_kind=3 carries both the slug and the provider label.
        // A regression dropping the label bytes from the encoder while
        // the decoder still reads them would only surface at a real
        // console-initiated provider launch — pin the round-trip here.
        let spawn = Some(SpawnRequest::AgentWithProvider {
            slug: "claude".to_string(),
            provider_label: "Z.AI".to_string(),
        });
        let bytes = encode_client(ClientFrame::Hello {
            rows: 50,
            cols: 200,
            spawn: spawn.clone(),
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .unwrap();
        let payload = bytes[5..].to_vec();
        match decode_client(TAG_HELLO, payload).unwrap() {
            ClientFrame::Hello { spawn: out, .. } => assert_eq!(out, spawn),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn hello_rejects_oversized_provider_label_at_encode() {
        let err = encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: Some(SpawnRequest::AgentWithProvider {
                slug: "claude".to_string(),
                provider_label: "p".repeat(MAX_HELLO_PROVIDER_LABEL + 1),
            }),
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .expect_err("over-cap provider label must be rejected at encode");
        let msg = format!("{err:#}");
        assert!(msg.contains("provider label"), "got: {msg}");
        assert!(
            msg.contains(&MAX_HELLO_PROVIDER_LABEL.to_string()),
            "got: {msg}"
        );
    }

    #[test]
    fn hello_rejects_empty_provider_label_at_decode() {
        // spawn_kind=3, slug="claude", provider_label_len=0. The decoder
        // must reject an AgentWithProvider frame with no label rather than
        // construct one the daemon would route as an unknown provider.
        let mut payload = vec![0, 24, 0, 80, 3, 0, 6];
        payload.extend(b"claude");
        payload.extend_from_slice(&0u16.to_be_bytes()); // provider_label_len = 0
        assert!(decode_client(TAG_HELLO, payload).is_err());
    }

    #[test]
    fn hello_rejects_oversized_agent_len() {
        // spawn_kind=agent, agent_len=99 but payload only carries
        // 12 bytes of "only-7-bytes".
        // decode must bail rather than slice past the buffer.
        let mut payload = vec![0, 42, 0, 100, 2, 0, 99];
        payload.extend(b"only-7-bytes");
        assert!(decode_client(TAG_HELLO, payload).is_err());
    }

    #[test]
    fn hello_rejects_non_utf8_agent_bytes() {
        let mut payload = vec![0, 42, 0, 100, 2, 0, 3];
        payload.extend(&[0xFF, 0xFE, 0xFD]);
        assert!(decode_client(TAG_HELLO, payload).is_err());
    }

    #[test]
    fn hello_rejects_truncated_env_value() {
        let mut payload = vec![0, 42, 0, 100, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 99];
        payload.extend(b"KEY");
        payload.extend(b"short");
        assert!(decode_client(TAG_HELLO, payload).is_err());
    }

    #[test]
    fn hello_rejects_truncated_4_byte_payload() {
        let payload = vec![0, 24, 0, 80];
        assert!(decode_client(TAG_HELLO, payload).is_err());
    }

    #[test]
    fn hello_shell_with_non_empty_agent_slug_rejected() {
        // spawn_kind=1 (Shell), agent_len=5 ("claude"-ish bytes).
        // Shell + slug is structurally invalid; decode must bail.
        let mut payload = vec![0, 24, 0, 80, 1, 0, 5];
        payload.extend(b"claud");
        payload.extend(&[0, 0]);
        payload.push(0);
        assert!(decode_client(TAG_HELLO, payload).is_err());
    }

    #[test]
    fn hello_with_trailing_bytes_rejected() {
        // Extra byte after the focus_kind tail must fail rather than be
        // tolerated — the wire format is closed, future fields go via a
        // versioned schema bump.
        let mut bytes = encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .expect("encode_client for a valid Hello must succeed");
        bytes.push(0xFF);
        let payload = bytes[5..].to_vec();
        assert!(decode_client(TAG_HELLO, payload).is_err());
    }

    #[test]
    fn welcome_decodes_session_count() {
        let bytes = encode_server(ServerFrame::Welcome { session_count: 7 });
        let payload = bytes[5..].to_vec();
        let frame = decode_server(TAG_WELCOME, payload).unwrap();
        assert_eq!(frame, ServerFrame::Welcome { session_count: 7 });
    }

    #[test]
    fn welcome_rejects_truncated_payload() {
        assert!(decode_server(TAG_WELCOME, vec![0, 0]).is_err());
    }

    #[test]
    fn server_frames_roundtrip() {
        for frame in [
            ServerFrame::Output(b"raw bytes".to_vec()),
            ServerFrame::SessionList(br#"[{"id":1}]"#.to_vec()),
            ServerFrame::Shutdown,
            ServerFrame::Bell,
        ] {
            let bytes = encode_server(frame.clone());
            let tag = bytes[0];
            let payload = bytes[5..].to_vec();
            assert_eq!(decode_server(tag, payload).unwrap(), frame);
        }
    }

    #[test]
    fn unknown_server_tag_rejected() {
        assert!(decode_server(0xFE, Vec::new()).is_err());
    }

    #[test]
    fn read_client_frame_rejects_oversize() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut a, mut b) = UnixStream::pair().unwrap();
            let oversize_len = (MAX_FRAME_PAYLOAD + 1) as u32;
            a.write_all(&oversize_len.to_be_bytes()).await.unwrap();
            a.shutdown().await.unwrap();
            let result = read_client_frame(&mut b, TAG_INPUT).await;
            assert!(
                result.is_err(),
                "expected oversize rejection, got {result:?}"
            );
        });
    }

    #[test]
    fn read_client_frame_accepts_exact_max_payload() {
        // Boundary partner for `read_client_frame_rejects_oversize`: a
        // refactor that swaps the inequality from `>` to `>=` in
        // `read_framed_payload` would silently shrink the documented
        // maximum by one byte. This test fails the moment that happens.
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut a, mut b) = UnixStream::pair().unwrap();
            let exact_len = MAX_FRAME_PAYLOAD as u32;
            let write_task = tokio::spawn(async move {
                a.write_all(&exact_len.to_be_bytes()).await.unwrap();
                a.write_all(&vec![0x42u8; MAX_FRAME_PAYLOAD]).await.unwrap();
                a.shutdown().await.unwrap();
            });
            let result = read_client_frame(&mut b, TAG_INPUT).await;
            write_task.await.unwrap();
            let frame = result
                .expect("must not reject exact-max payload")
                .expect("frame present");
            match frame {
                ClientFrame::Input(bytes) => assert_eq!(bytes.len(), MAX_FRAME_PAYLOAD),
                other => panic!("expected Input, got {other:?}"),
            }
        });
    }

    #[test]
    fn hello_env_count_over_cap_is_rejected_by_encoder() {
        // Encoder gate must reject `MAX_HELLO_ENV + 1`. Without this the
        // wire could carry an env list a future decoder gladly accepts,
        // bypassing the documented cap.
        let env: Vec<(String, String)> = (0..=MAX_HELLO_ENV)
            .map(|i| (format!("K{i}"), "v".into()))
            .collect();
        let err = encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: None,
            env,
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .expect_err("over-cap env must be rejected at encode");
        let msg = format!("{err:#}");
        assert!(msg.contains("env count"), "got: {msg}");
        assert!(msg.contains(&MAX_HELLO_ENV.to_string()), "got: {msg}");
    }

    #[test]
    fn hello_env_count_over_cap_is_rejected_by_decoder() {
        // Decoder must refuse a hand-crafted payload claiming
        // `env_count = MAX_HELLO_ENV + 1`. This is the wire-level
        // counterpart of the encoder guard: a buggy or hostile peer
        // could otherwise force the daemon to pre-allocate an
        // arbitrarily large env table.
        let mut payload = Vec::new();
        payload.extend_from_slice(&24u16.to_be_bytes()); // rows
        payload.extend_from_slice(&80u16.to_be_bytes()); // cols
        payload.push(0u8); // spawn_kind = None
        payload.extend_from_slice(&0u16.to_be_bytes()); // agent_len = 0
        let bogus_count = u16::try_from(MAX_HELLO_ENV + 1).expect("fits u16");
        payload.extend_from_slice(&bogus_count.to_be_bytes());
        let err = decode_client(TAG_HELLO, payload)
            .expect_err("over-cap env_count must be rejected at decode");
        let msg = format!("{err:#}");
        assert!(msg.contains("env_count"), "got: {msg}");
        assert!(msg.contains(&MAX_HELLO_ENV.to_string()), "got: {msg}");
    }

    #[test]
    fn hello_env_count_over_cap_is_rejected_by_decoder_with_full_payload() {
        // Partner for `hello_env_count_over_cap_is_rejected_by_decoder`:
        // that test crafts ONLY the env_count and stops, so the
        // front-of-loop guard fires before the per-entry read runs. A
        // refactor that moved the cap check below the per-entry loop
        // (computing it from accumulated reads) would still pass that
        // test. This variant supplies a fully-populated payload of
        // `MAX_HELLO_ENV + 1` real entries so the boundary is verified
        // after the per-entry read, not just at the count declaration.
        let mut payload = Vec::new();
        payload.extend_from_slice(&24u16.to_be_bytes()); // rows
        payload.extend_from_slice(&80u16.to_be_bytes()); // cols
        payload.push(0u8); // spawn_kind = None
        payload.extend_from_slice(&0u16.to_be_bytes()); // agent_len = 0
        let bogus_count = u16::try_from(MAX_HELLO_ENV + 1).expect("fits u16");
        payload.extend_from_slice(&bogus_count.to_be_bytes());
        for i in 0..=MAX_HELLO_ENV {
            let key = format!("K{i}");
            let value = "v";
            payload.extend_from_slice(&(key.len() as u16).to_be_bytes());
            payload.extend_from_slice(&(value.len() as u32).to_be_bytes());
            payload.extend_from_slice(key.as_bytes());
            payload.extend_from_slice(value.as_bytes());
        }
        payload.push(0u8); // focus_kind = None
        let err = decode_client(TAG_HELLO, payload)
            .expect_err("fully-populated over-cap env_count must be rejected");
        let msg = format!("{err:#}");
        assert!(msg.contains("env_count"), "got: {msg}");
        assert!(msg.contains(&MAX_HELLO_ENV.to_string()), "got: {msg}");
    }

    #[test]
    fn hello_env_count_at_cap_round_trips() {
        // Partner for `hello_env_count_over_cap_is_rejected_by_encoder`:
        // a refactor that swaps `>` to `>=` in the encoder OR decoder
        // would silently shrink the documented cap. Both sides must
        // accept exactly `MAX_HELLO_ENV` entries.
        let env: Vec<(String, String)> = (0..MAX_HELLO_ENV)
            .map(|i| (format!("K{i}"), "v".into()))
            .collect();
        let bytes = encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: None,
            env: env.clone(),
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .expect("at-cap env must encode");
        let payload = bytes[5..].to_vec();
        let decoded = decode_client(TAG_HELLO, payload).expect("at-cap env must decode");
        match decoded {
            ClientFrame::Hello { env: out, .. } => assert_eq!(out, env),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn hello_with_focus_session_round_trips() {
        // The console preview-pane click path sets
        // `focus_session: Some(<session_id>)`. A refactor that drops
        // the trailing 8 bytes of session id from the encoder while
        // the decoder still expects them would only fail at a real
        // attach. Exercise the round-trip explicitly so the contract
        // is pinned in the test suite.
        let target = 0xDEAD_BEEF_CAFE_BABEu64;
        let bytes = encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: None,
            env: Vec::new(),
            terminal: ClientTerminal::default(),
            focus_session: Some(target),
        })
        .expect("focus_session encode");
        let payload = bytes[5..].to_vec();
        let decoded = decode_client(TAG_HELLO, payload).expect("focus_session decode");
        match decoded {
            ClientFrame::Hello { focus_session, .. } => {
                assert_eq!(focus_session, Some(target));
            }
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn hello_with_client_terminal_round_trips() {
        let terminal = ClientTerminal {
            term: Some("xterm-ghostty".to_string()),
            term_program: Some("ghostty".to_string()),
            colorterm: Some("truecolor".to_string()),
        };
        let bytes = encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: None,
            env: Vec::new(),
            terminal: terminal.clone(),
            focus_session: None,
        })
        .expect("terminal identity encode");
        let payload = bytes[5..].to_vec();
        let decoded = decode_client(TAG_HELLO, payload).expect("terminal identity decode");
        match decoded {
            ClientFrame::Hello { terminal: out, .. } => assert_eq!(out, terminal),
            other => panic!("expected Hello, got {other:?}"),
        }
    }

    #[test]
    fn client_terminal_detects_known_pointer_shape_support() {
        let ghostty = ClientTerminal {
            term: Some("xterm-ghostty".to_string()),
            ..ClientTerminal::default()
        };
        let kitty = ClientTerminal {
            term: Some("xterm-kitty".to_string()),
            ..ClientTerminal::default()
        };
        let iterm = ClientTerminal {
            term_program: Some("iTerm.app".to_string()),
            ..ClientTerminal::default()
        };
        let warp = ClientTerminal {
            term_program: Some("WarpTerminal".to_string()),
            ..ClientTerminal::default()
        };
        let apple_terminal = ClientTerminal {
            term: Some("xterm-256color".to_string()),
            term_program: Some("Apple_Terminal".to_string()),
            ..ClientTerminal::default()
        };
        let generic_xterm = ClientTerminal {
            term: Some("xterm-256color".to_string()),
            ..ClientTerminal::default()
        };
        let dumb = ClientTerminal {
            term: Some("dumb".to_string()),
            ..ClientTerminal::default()
        };

        assert!(ghostty.pointer_shapes_supported());
        assert!(kitty.pointer_shapes_supported());
        assert!(iterm.pointer_shapes_supported());
        assert!(apple_terminal.pointer_shapes_supported());
        assert!(!generic_xterm.pointer_shapes_supported());
        assert!(!warp.pointer_shapes_supported());
        assert!(!dumb.pointer_shapes_supported());
    }

    #[test]
    fn hello_env_value_over_cap_rejected_by_encoder() {
        // Encoder gate must reject a single env value larger than
        // MAX_HELLO_ENV_VALUE so a buggy producer cannot smuggle a
        // megabyte-sized env entry past MAX_HELLO_ENV.
        let big = "v".repeat(MAX_HELLO_ENV_VALUE + 1);
        let err = encode_client(ClientFrame::Hello {
            rows: 24,
            cols: 80,
            spawn: None,
            env: vec![("PWD".into(), big)],
            terminal: ClientTerminal::default(),
            focus_session: None,
        })
        .expect_err("over-cap env value must be rejected at encode");
        let msg = format!("{err:#}");
        assert!(msg.contains("env value"), "got: {msg}");
        assert!(msg.contains(&MAX_HELLO_ENV_VALUE.to_string()), "got: {msg}");
    }

    #[test]
    fn hello_env_value_over_cap_rejected_by_decoder() {
        // Wire-level counterpart: a hand-crafted payload claiming
        // value_len > MAX_HELLO_ENV_VALUE must be rejected before
        // any read_string allocates the actual bytes.
        let mut payload = Vec::new();
        payload.extend_from_slice(&24u16.to_be_bytes()); // rows
        payload.extend_from_slice(&80u16.to_be_bytes()); // cols
        payload.push(0u8); // spawn_kind = None
        payload.extend_from_slice(&0u16.to_be_bytes()); // agent_len = 0
        payload.extend_from_slice(&1u16.to_be_bytes()); // env_count = 1
        payload.extend_from_slice(&3u16.to_be_bytes()); // key_len = 3
        let bogus_value_len = u32::try_from(MAX_HELLO_ENV_VALUE + 1).expect("fits u32");
        payload.extend_from_slice(&bogus_value_len.to_be_bytes());
        payload.extend_from_slice(b"PWD");
        // No need to supply the value bytes; the cap check fires before
        // read_string reaches into the buffer.
        let err = decode_client(TAG_HELLO, payload)
            .expect_err("over-cap env value length must be rejected at decode");
        let msg = format!("{err:#}");
        assert!(msg.contains("env value"), "got: {msg}");
        assert!(msg.contains(&MAX_HELLO_ENV_VALUE.to_string()), "got: {msg}");
    }

    #[test]
    fn hello_env_key_over_cap_rejected_by_decoder() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&24u16.to_be_bytes());
        payload.extend_from_slice(&80u16.to_be_bytes());
        payload.push(0u8);
        payload.extend_from_slice(&0u16.to_be_bytes());
        payload.extend_from_slice(&1u16.to_be_bytes());
        let bogus_key_len = u16::try_from(MAX_HELLO_ENV_KEY + 1).expect("fits u16");
        payload.extend_from_slice(&bogus_key_len.to_be_bytes());
        payload.extend_from_slice(&1u32.to_be_bytes());
        let err = decode_client(TAG_HELLO, payload)
            .expect_err("over-cap env key length must be rejected at decode");
        let msg = format!("{err:#}");
        assert!(msg.contains("env key"), "got: {msg}");
        assert!(msg.contains(&MAX_HELLO_ENV_KEY.to_string()), "got: {msg}");
    }

    #[test]
    fn read_client_frame_eof_after_tag_returns_none() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (mut a, mut b) = UnixStream::pair().unwrap();
            // Tag is treated as already-peeked; write nothing else, then
            // close. The reader should hit EOF inside the length read
            // and return Ok(None), not Err.
            a.shutdown().await.unwrap();
            drop(a);
            let result = read_client_frame(&mut b, TAG_INPUT).await.unwrap();
            assert!(result.is_none());
        });
    }
}
