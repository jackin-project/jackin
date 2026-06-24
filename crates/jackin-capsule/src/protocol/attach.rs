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
/// env entry past `MAX_HELLO_ENV` (the count cap) into the spawned
/// session's environment block.
pub const MAX_HELLO_ENV_VALUE: usize = 8 * 1024;
/// Per-entry cap on Hello env-key byte length. Same shape as
/// `MAX_HELLO_ENV_VALUE`; env-var names should be even shorter than
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
    /// as the tab suffix instead of showing the in-mux `ProviderPicker` dialog.
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
    pub fn agent(slug: impl Into<String>) -> Result<Self> {
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
    /// Default foreground/background the client read from its terminal via
    /// OSC 10/11 before the handshake. `None` when the terminal did not
    /// answer. The daemon feeds these into every pane grid so agent OSC 10/11
    /// queries are answered with the colors the operator actually sees.
    pub default_fg: Option<(u8, u8, u8)>,
    pub default_bg: Option<(u8, u8, u8)>,
}

/// Backend-side capabilities for the currently attached terminal.
///
/// These are host-adaptive and may change on every attach. They must not
/// change agent-visible `jackin-term` profile semantics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttachCapabilities {
    pub pointer_shapes: bool,
    pub truecolor: bool,
    pub synchronized_output: bool,
    pub osc8_hyperlinks: bool,
    pub underline_style: bool,
    pub image_protocol: ImageProtocolCapability,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImageProtocolCapability {
    #[default]
    Unsupported,
    Kitty,
}

impl ClientTerminal {
    pub fn from_env() -> Self {
        Self {
            term: non_empty_env(TERM_LABEL),
            term_program: non_empty_env(TERM_PROGRAM_LABEL),
            colorterm: non_empty_env(COLORTERM_LABEL),
            default_fg: None,
            default_bg: None,
        }
    }

    #[must_use]
    pub fn pointer_shapes_supported(&self) -> bool {
        self.attach_capabilities().pointer_shapes
    }

    #[must_use]
    pub fn attach_capabilities(&self) -> AttachCapabilities {
        let term = self.term.as_deref().unwrap_or("").to_ascii_lowercase();
        let term_program = self
            .term_program
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase();

        let known_terminal = !(term.is_empty() && term_program.is_empty())
            && !matches!(term.as_str(), "dumb" | "linux");
        let warp = term_program.contains("warp");
        let ghostty = term.contains("ghostty") || term_program.contains("ghostty");
        let kitty = term.contains("kitty") || term_program.contains("kitty");
        let wezterm = term.contains("wezterm") || term_program.contains("wezterm");
        let iterm = term_program.contains("iterm");
        let apple_terminal = term_program.contains("apple_terminal");
        let truecolor = self.colorterm.as_deref().is_some_and(|value| {
            matches!(value.to_ascii_lowercase().as_str(), "truecolor" | "24bit")
        });

        let pointer_shapes = known_terminal
            && !warp
            && (!term_program.is_empty()
                || term.contains("ghostty")
                || term.contains("kitty")
                || term.contains("foot"));

        AttachCapabilities {
            pointer_shapes,
            truecolor,
            synchronized_output: known_terminal,
            osc8_hyperlinks: known_terminal && !matches!(term.as_str(), "linux" | "dumb"),
            underline_style: ghostty || kitty || wezterm || iterm || apple_terminal,
            image_protocol: if kitty {
                ImageProtocolCapability::Kitty
            } else {
                ImageProtocolCapability::Unsupported
            },
        }
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
    Shutdown { reason: Option<String> },
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
        // The wire payload is the reason bytes; an empty payload means "no
        // reason". `Some("")` therefore round-trips back as `None` on decode —
        // no caller emits an empty reason (all are non-empty or genuine
        // `None`), so this normalization is safe, not lossy in practice.
        ServerFrame::Shutdown { reason } => encode(
            TAG_SHUTDOWN,
            reason.as_deref().unwrap_or_default().as_bytes(),
        ),
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
            //   fg_present(1) [fg_r(1) fg_g(1) fg_b(1) if 1]
            //   bg_present(1) [bg_r(1) bg_g(1) bg_b(1) if 1]
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
            write_color_field(&mut payload, terminal.default_fg);
            write_color_field(&mut payload, terminal.default_bg);
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

fn write_color_field(payload: &mut Vec<u8>, color: Option<(u8, u8, u8)>) {
    match color {
        None => payload.push(0u8),
        Some((r, g, b)) => {
            payload.push(1u8);
            payload.extend_from_slice(&[r, g, b]);
        }
    }
}

fn read_color_field(cursor: &mut PayloadCursor<'_>, label: &str) -> Result<Option<(u8, u8, u8)>> {
    let kind_label = format!("{label} presence");
    match cursor.read_u8(&kind_label)? {
        0 => Ok(None),
        1 => {
            let r = cursor.read_u8(&format!("{label} r"))?;
            let g = cursor.read_u8(&format!("{label} g"))?;
            let b = cursor.read_u8(&format!("{label} b"))?;
            Ok(Some((r, g, b)))
        }
        other => bail!("unknown hello {label} presence byte {other}"),
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
                default_fg: read_color_field(&mut cursor, "default fg")?,
                default_bg: read_color_field(&mut cursor, "default bg")?,
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
        TAG_SHUTDOWN => {
            let reason = if payload.is_empty() {
                None
            } else {
                Some(String::from_utf8(payload).context("shutdown reason is not UTF-8")?)
            };
            ServerFrame::Shutdown { reason }
        }
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
        Ok(s.to_owned())
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
mod tests;
