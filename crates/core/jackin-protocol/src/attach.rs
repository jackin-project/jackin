#![expect(
    clippy::too_many_lines,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]

use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::TelemetryContext;

mod contextual;

pub const TAG_HELLO: u8 = 0x01;
pub const TAG_RESIZE: u8 = 0x02;
pub const TAG_INPUT: u8 = 0x03;
pub const TAG_COMMAND: u8 = 0x04;
pub const TAG_DETACH: u8 = 0x05;
pub const TAG_FOCUS_IN: u8 = 0x06;
pub const TAG_FOCUS_OUT: u8 = 0x07;
pub const TAG_CLIPBOARD_IMAGE: u8 = 0x08;
pub const TAG_CLIPBOARD_IMAGE_START: u8 = 0x09;
pub const TAG_CLIPBOARD_IMAGE_CHUNK: u8 = 0x0a;
pub const TAG_CLIPBOARD_IMAGE_END: u8 = 0x0b;
pub const TAG_CLIPBOARD_IMAGE_ERROR: u8 = 0x0c;
pub const TAG_HOST_NOTICE: u8 = 0x0d;
pub const TAG_ATTACH_CONTROL: u8 = 0x0e;

pub const TAG_WELCOME: u8 = 0x81;
pub const TAG_OUTPUT: u8 = 0x82;
pub const TAG_SESSION_LIST: u8 = 0x83;
pub const TAG_SHUTDOWN: u8 = 0x84;
pub const TAG_BELL: u8 = 0x85;
pub const TAG_HOST_OPEN_URL: u8 = 0x86;
pub const TAG_FILE_EXPORT_START: u8 = 0x87;
pub const TAG_FILE_EXPORT_CHUNK: u8 = 0x88;
pub const TAG_FILE_EXPORT_END: u8 = 0x89;
pub const TAG_HOST_STAGE_IMAGE_FROM_CLIPBOARD_PATH: u8 = 0x8a;
pub const TAG_HOST_PASTE_IMAGE_FROM_CLIPBOARD: u8 = 0x8b;
pub const TAG_HOST_STAGE_IMAGE_FROM_CLIPBOARD: u8 = 0x8c;
pub const TAG_HOST_REVEAL_PATH: u8 = 0x8d;
pub const TAG_ATTACH_CONTROL_RESPONSE: u8 = 0x8e;

const MAX_FRAME_PAYLOAD: usize = 4 * 1024 * 1024;
const MAX_CLIPBOARD_IMAGE_FRAME_PAYLOAD: usize = 16 * 1024 * 1024;
pub const MAX_FILE_EXPORT_PATH_BYTES: usize = 4096;
pub const MAX_FILE_EXPORT_NAME_BYTES: usize = 255;
pub const MAX_FILE_EXPORT_CHUNK_BYTES: usize = 1024 * 1024;
pub const FILE_EXPORT_DIGEST_BYTES: usize = 32;
pub const MAX_CLIPBOARD_IMAGE_ERROR_BYTES: usize = 1024;
pub const MAX_HOST_NOTICE_BYTES: usize = 2048;
pub const MAX_HOST_REVEAL_PATH_BYTES: usize = 4096;
pub const MAX_CLIPBOARD_IMAGE_CHUNK_BYTES: usize = 1024 * 1024;
pub const MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES_U64: u64 = MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES as u64;
pub const MAX_CLIPBOARD_IMAGE_BYTES: usize = MAX_CLIPBOARD_IMAGE_FRAME_PAYLOAD - 1;
pub const MAX_CONTEXTUAL_CLIPBOARD_IMAGE_BYTES: usize = MAX_CLIPBOARD_IMAGE_BYTES - 2048;
pub const MAX_HELLO_ENV: usize = 64;
pub const MAX_HELLO_ENV_VALUE: usize = 8 * 1024;
pub const MAX_HELLO_ENV_KEY: usize = 1024;
pub const MAX_CLIENT_TERMINAL_FIELD: usize = 1024;

const TERM_LABEL: &str = "TERM";
const TERM_PROGRAM_LABEL: &str = "TERM_PROGRAM";
const COLORTERM_LABEL: &str = "COLORTERM";

const FRAME_READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnRequest {
    Shell,
    Instance(String),
}

impl SpawnRequest {
    pub fn instance(target: impl Into<String>) -> Result<Self> {
        let target = target.into();
        if target.is_empty() {
            anyhow::bail!("SpawnRequest::Instance target must be non-empty");
        }
        Ok(SpawnRequest::Instance(target))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientTerminal {
    pub term: Option<String>,
    pub term_program: Option<String>,
    pub colorterm: Option<String>,
    pub default_fg: Option<(u8, u8, u8)>,
    pub default_bg: Option<(u8, u8, u8)>,
    pub capability_overrides: AttachCapabilityOverrides,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Orthogonal terminal capability flags (pointer_shapes, truecolor, \
              synchronized_output, osc8_hyperlinks, underline_style) — each is an \
              independent feature bit resolved from the TERM / TERM_PROGRAM / \
              COLORTERM / capability overrides, consumed individually by downstream \
              capability gates. Bundling into bitflags would lose naming at the read \
              site without changing observable behavior."
)]
pub struct AttachCapabilities {
    pub pointer_shapes: bool,
    pub truecolor: bool,
    pub synchronized_output: bool,
    pub osc8_hyperlinks: bool,
    pub underline_style: bool,
    pub image_protocol: ImageProtocolCapability,
    pub sources: AttachCapabilitySources,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Per-capability source-of-truth flags (handshake_identity, terminfo_name, \
              safe_color_probe, user_override, denylist) — each is an independent \
              provenance bit that the operator inspects to understand why a capability \
              resolved the way it did. Named-field reads are clearer than bit-position \
              lookups in capability-debug output."
)]
pub struct AttachCapabilitySources {
    pub handshake_identity: bool,
    pub terminfo_name: bool,
    pub safe_color_probe: bool,
    pub user_override: bool,
    pub denylist: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttachCapabilityOverrides {
    pub pointer_shapes: Option<bool>,
    pub truecolor: Option<bool>,
    pub synchronized_output: Option<bool>,
    pub osc8_hyperlinks: Option<bool>,
    pub underline_style: Option<bool>,
    pub image_protocol: Option<ImageProtocolCapability>,
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
            capability_overrides: AttachCapabilityOverrides::from_env(),
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

        let known_terminal = !(matches!(term.as_str(), "dumb" | "linux")
            || term.is_empty() && term_program.is_empty());
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

        let denylist = !known_terminal || warp || matches!(term.as_str(), "dumb" | "linux");
        let sources = AttachCapabilitySources {
            handshake_identity: known_terminal,
            terminfo_name: !term.is_empty(),
            safe_color_probe: self.default_fg.is_some() || self.default_bg.is_some(),
            user_override: self.capability_overrides.any(),
            denylist,
        };
        let mut caps = AttachCapabilities {
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
            sources,
        };
        self.capability_overrides.apply_to(&mut caps);
        caps
    }
}

impl AttachCapabilityOverrides {
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            pointer_shapes: env_bool("JACKIN_ATTACH_POINTER_SHAPES"),
            truecolor: env_bool("JACKIN_ATTACH_TRUECOLOR"),
            synchronized_output: env_bool("JACKIN_ATTACH_SYNC"),
            osc8_hyperlinks: env_bool("JACKIN_ATTACH_OSC8"),
            underline_style: env_bool("JACKIN_ATTACH_UNDERLINE_STYLE"),
            image_protocol: env_image_protocol("JACKIN_ATTACH_IMAGE_PROTOCOL"),
        }
    }

    #[must_use]
    pub const fn any(self) -> bool {
        self.pointer_shapes.is_some()
            || self.truecolor.is_some()
            || self.synchronized_output.is_some()
            || self.osc8_hyperlinks.is_some()
            || self.underline_style.is_some()
            || self.image_protocol.is_some()
    }

    fn apply_to(self, caps: &mut AttachCapabilities) {
        if let Some(value) = self.pointer_shapes {
            caps.pointer_shapes = value;
        }
        if let Some(value) = self.truecolor {
            caps.truecolor = value;
        }
        if let Some(value) = self.synchronized_output {
            caps.synchronized_output = value;
        }
        if let Some(value) = self.osc8_hyperlinks {
            caps.osc8_hyperlinks = value;
        }
        if let Some(value) = self.underline_style {
            caps.underline_style = value;
        }
        if let Some(value) = self.image_protocol {
            caps.image_protocol = value;
        }
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn env_bool(key: &str) -> Option<bool> {
    match std::env::var(key).as_deref() {
        Ok("1" | "true" | "yes" | "on") => Some(true),
        Ok("0" | "false" | "no" | "off" | "deny") => Some(false),
        _ => None,
    }
}

fn env_image_protocol(key: &str) -> Option<ImageProtocolCapability> {
    match std::env::var(key).as_deref() {
        Ok("kitty") => Some(ImageProtocolCapability::Kitty),
        Ok("unsupported" | "none" | "off" | "deny") => Some(ImageProtocolCapability::Unsupported),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Tiff,
}

impl ClipboardImageFormat {
    #[must_use]
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Gif => "gif",
            Self::Webp => "webp",
            Self::Tiff => "tiff",
        }
    }

    fn tag(&self) -> u8 {
        match self {
            Self::Png => 1,
            Self::Jpeg => 2,
            Self::Gif => 3,
            Self::Webp => 4,
            Self::Tiff => 5,
        }
    }

    fn from_tag(tag: u8) -> Result<Self> {
        Ok(match tag {
            1 => Self::Png,
            2 => Self::Jpeg,
            3 => Self::Gif,
            4 => Self::Webp,
            5 => Self::Tiff,
            other => bail!("unknown clipboard image format tag {other}"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    pub format: ClipboardImageFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImageStart {
    pub transfer_id: u64,
    pub format: ClipboardImageFormat,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImageChunk {
    pub transfer_id: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImageEnd {
    pub transfer_id: u64,
    pub sha256: [u8; FILE_EXPORT_DIGEST_BYTES],
}

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachControlRequest {
    pub request_id: u64,
    pub context: TelemetryContext,
    pub operation: AttachControlOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachControlOperation {
    Detach,
    FocusIn,
    FocusOut,
    ClipboardImage(ClipboardImage),
    ClipboardImageStart(ClipboardImageStart),
    ClipboardImageChunk(ClipboardImageChunk),
    ClipboardImageEnd(ClipboardImageEnd),
    ClipboardImageError(ClipboardImageError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachControlResult {
    Success,
    InvalidCorrelation,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachControlResponse {
    pub request_id: u64,
    pub result: AttachControlResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExportStart {
    pub transfer_id: u64,
    pub source_path: String,
    pub file_name: String,
    pub size: u64,
    pub reveal_after_export: bool,
    pub open_after_export: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExportChunk {
    pub transfer_id: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileExportEnd {
    pub transfer_id: u64,
    pub sha256: [u8; FILE_EXPORT_DIGEST_BYTES],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardImageError {
    Empty,
    TooLarge,
    UnsupportedFormat,
    DigestMismatch,
    ChunkSequence,
    MissingTransfer,
    DuplicateTransfer,
    BackendUnavailable,
    Io,
    Other(String),
}

impl ClipboardImageError {
    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            Self::Empty => "clipboard image transfer is empty",
            Self::TooLarge => "clipboard image transfer exceeds cap",
            Self::UnsupportedFormat => {
                "clipboard image magic bytes do not match a supported format"
            }
            Self::DigestMismatch => "clipboard image transfer SHA-256 mismatch",
            Self::ChunkSequence => "clipboard image transfer chunk offset did not match expected",
            Self::MissingTransfer => "clipboard image transfer has no active start",
            Self::DuplicateTransfer => "clipboard image transfer already active",
            Self::BackendUnavailable => "host clipboard backend unavailable (xclip/wl-paste)",
            Self::Io => "clipboard image host I/O failed while reading/writing",
            Self::Other(message) => message.as_str(),
        }
    }

    #[must_use]
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLarge => "oversize",
            Self::UnsupportedFormat => "signature-mismatch",
            Self::DigestMismatch => "digest-mismatch",
            Self::ChunkSequence => "offset-mismatch",
            Self::MissingTransfer => "missing-transfer",
            Self::DuplicateTransfer => "duplicate-transfer",
            Self::BackendUnavailable => "backend-unavailable",
            Self::Io => "io",
            Self::Other(_) => "other",
        }
    }

    #[must_use]
    pub fn from_message(message: String) -> Self {
        let lower = message.to_ascii_lowercase();
        if lower.contains("empty") {
            Self::Empty
        } else if lower.contains("exceeds cap")
            || lower.contains("too large")
            || lower.contains("over cap")
        {
            Self::TooLarge
        } else if lower.contains("magic")
            || lower.contains("signature")
            || lower.contains("not an image")
            || lower.contains("unsupported image")
        {
            Self::UnsupportedFormat
        } else if lower.contains("sha-256") || lower.contains("digest") {
            Self::DigestMismatch
        } else if lower.contains("offset") || lower.contains("did not match expected") {
            Self::ChunkSequence
        } else if lower.contains("no active start") {
            Self::MissingTransfer
        } else if lower.contains("already active") {
            Self::DuplicateTransfer
        } else if lower.contains("display")
            || lower.contains("wayland")
            || lower.contains("xclip")
            || lower.contains("wl-paste")
            || lower.contains("wl-copy")
        {
            Self::BackendUnavailable
        } else if lower.contains("create")
            || lower.contains("creating")
            || lower.contains("open")
            || lower.contains("opening")
            || lower.contains("write")
            || lower.contains("writing")
            || lower.contains("flush")
            || lower.contains("flushing")
            || lower.contains("permission")
            || lower.contains("metadata")
            || lower.contains("read")
            || lower.contains("reading")
        {
            Self::Io
        } else {
            Self::Other(message)
        }
    }
}

impl std::fmt::Display for ClipboardImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientFrame {
    Hello {
        rows: u16,
        cols: u16,
        spawn: Option<SpawnRequest>,
        env: Vec<(String, String)>,
        focus_session: Option<u64>,
        terminal: ClientTerminal,
        context: Option<Box<TelemetryContext>>,
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
    ClipboardImage(ClipboardImage),
    ClipboardImageStart(ClipboardImageStart),
    ClipboardImageChunk(ClipboardImageChunk),
    ClipboardImageEnd(ClipboardImageEnd),
    ClipboardImageError(ClipboardImageError),
    HostNotice(String),
    AttachControl(AttachControlRequest),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerFrame {
    Welcome { session_count: u32 },

    Output(Vec<u8>),
    SessionList(Vec<u8>),

    Shutdown { reason: Option<String> },

    Bell,
    HostOpenUrl(String),
    FileExportStart(FileExportStart),
    FileExportChunk(FileExportChunk),
    FileExportEnd(FileExportEnd),
    HostRevealPath(String),
    HostStageImageFromClipboardPath,
    HostPasteImageFromClipboard,
    HostStageImageFromClipboard,
    AttachControlResponse(AttachControlResponse),
}

pub fn encode(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(tag);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn encode_server(frame: ServerFrame) -> Vec<u8> {
    match frame {
        ServerFrame::Welcome { session_count } => encode(TAG_WELCOME, &session_count.to_be_bytes()),
        ServerFrame::Output(bytes) => encode(TAG_OUTPUT, &bytes),
        ServerFrame::SessionList(json) => encode(TAG_SESSION_LIST, &json),
        ServerFrame::Shutdown { reason } => encode(
            TAG_SHUTDOWN,
            reason.as_deref().unwrap_or_default().as_bytes(),
        ),
        ServerFrame::Bell => encode(TAG_BELL, &[]),
        ServerFrame::HostOpenUrl(url) => encode(TAG_HOST_OPEN_URL, url.as_bytes()),
        ServerFrame::FileExportStart(start) => encode_file_export_start(start),
        ServerFrame::FileExportChunk(chunk) => encode_file_export_chunk(chunk),
        ServerFrame::FileExportEnd(end) => encode_file_export_end(end),
        ServerFrame::HostRevealPath(path) => encode(TAG_HOST_REVEAL_PATH, path.as_bytes()),
        ServerFrame::HostStageImageFromClipboardPath => {
            encode(TAG_HOST_STAGE_IMAGE_FROM_CLIPBOARD_PATH, &[])
        }
        ServerFrame::HostPasteImageFromClipboard => {
            encode(TAG_HOST_PASTE_IMAGE_FROM_CLIPBOARD, &[])
        }
        ServerFrame::HostStageImageFromClipboard => {
            encode(TAG_HOST_STAGE_IMAGE_FROM_CLIPBOARD, &[])
        }
        ServerFrame::AttachControlResponse(response) => contextual::encode_response(response),
    }
}

fn encode_file_export_start(start: FileExportStart) -> Vec<u8> {
    let source = start.source_path.as_bytes();
    let name = start.file_name.as_bytes();
    assert!(!source.is_empty());
    assert!(source.len() <= MAX_FILE_EXPORT_PATH_BYTES);
    assert!(!name.is_empty());
    assert!(name.len() <= MAX_FILE_EXPORT_NAME_BYTES);
    let source_len = source.len() as u16;
    let name_len = name.len() as u16;
    let mut payload = Vec::with_capacity(22 + source.len() + name.len());
    payload.extend_from_slice(&start.transfer_id.to_be_bytes());
    payload.extend_from_slice(&start.size.to_be_bytes());
    payload.extend_from_slice(&source_len.to_be_bytes());
    payload.extend_from_slice(&name_len.to_be_bytes());
    payload.push(u8::from(start.reveal_after_export));
    payload.push(u8::from(start.open_after_export));
    payload.extend_from_slice(source);
    payload.extend_from_slice(name);
    encode(TAG_FILE_EXPORT_START, &payload)
}

fn encode_file_export_chunk(chunk: FileExportChunk) -> Vec<u8> {
    assert!(!chunk.bytes.is_empty());
    assert!(chunk.bytes.len() <= MAX_FILE_EXPORT_CHUNK_BYTES);
    let mut payload = Vec::with_capacity(16 + chunk.bytes.len());
    payload.extend_from_slice(&chunk.transfer_id.to_be_bytes());
    payload.extend_from_slice(&chunk.offset.to_be_bytes());
    payload.extend_from_slice(&chunk.bytes);
    encode(TAG_FILE_EXPORT_CHUNK, &payload)
}

fn encode_file_export_end(end: FileExportEnd) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8 + FILE_EXPORT_DIGEST_BYTES);
    payload.extend_from_slice(&end.transfer_id.to_be_bytes());
    payload.extend_from_slice(&end.sha256);
    encode(TAG_FILE_EXPORT_END, &payload)
}

pub fn encode_client(frame: ClientFrame) -> Result<Vec<u8>> {
    Ok(match frame {
        ClientFrame::Hello {
            rows,
            cols,
            spawn,
            env,
            focus_session,
            terminal,
            context,
        } => {
            let (spawn_kind, agent_bytes): (u8, &[u8]) = match spawn.as_ref() {
                None => (0, b""),
                Some(SpawnRequest::Shell) => (1, b""),
                Some(SpawnRequest::Instance(target)) => (2, target.as_bytes()),
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
            write_capability_overrides(&mut payload, terminal.capability_overrides);
            let context =
                serde_json::to_vec(&context).context("serializing Hello telemetry context")?;
            let context_len = u16::try_from(context.len())
                .map_err(|_| anyhow::anyhow!("Hello telemetry context exceeds u16::MAX bytes"))?;
            payload.extend_from_slice(&context_len.to_be_bytes());
            payload.extend_from_slice(&context);
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
        ClientFrame::ClipboardImageStart(start) => encode_clipboard_image_start(start),
        ClientFrame::ClipboardImageChunk(chunk) => encode_clipboard_image_chunk(chunk),
        ClientFrame::ClipboardImageEnd(end) => encode_clipboard_image_end(end),
        ClientFrame::ClipboardImageError(error) => {
            let message = error.message().as_bytes();
            if message.is_empty() {
                bail!("clipboard image error message is empty");
            }
            if message.len() > MAX_CLIPBOARD_IMAGE_ERROR_BYTES {
                bail!(
                    "clipboard image error message {} exceeds cap {MAX_CLIPBOARD_IMAGE_ERROR_BYTES}",
                    message.len()
                );
            }
            encode(TAG_CLIPBOARD_IMAGE_ERROR, message)
        }
        ClientFrame::HostNotice(message) => {
            let message = message.as_bytes();
            if message.is_empty() {
                bail!("host notice message is empty");
            }
            if message.len() > MAX_HOST_NOTICE_BYTES {
                bail!(
                    "host notice message {} exceeds cap {MAX_HOST_NOTICE_BYTES}",
                    message.len()
                );
            }
            encode(TAG_HOST_NOTICE, message)
        }
        ClientFrame::ClipboardImage(image) => {
            if image.bytes.is_empty() {
                bail!("clipboard image payload is empty");
            }
            if image.bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES {
                bail!(
                    "clipboard image payload {} exceeds cap {MAX_CLIPBOARD_IMAGE_BYTES}",
                    image.bytes.len()
                );
            }
            let mut payload = Vec::with_capacity(1 + image.bytes.len());
            payload.push(image.format.tag());
            payload.extend_from_slice(&image.bytes);
            encode(TAG_CLIPBOARD_IMAGE, &payload)
        }
        ClientFrame::AttachControl(request) => contextual::encode_request(request)?,
    })
}

fn encode_clipboard_image_start(start: ClipboardImageStart) -> Vec<u8> {
    assert!(start.size > 0);
    assert!(start.size <= MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES_U64);
    let mut payload = Vec::with_capacity(17);
    payload.extend_from_slice(&start.transfer_id.to_be_bytes());
    payload.push(start.format.tag());
    payload.extend_from_slice(&start.size.to_be_bytes());
    encode(TAG_CLIPBOARD_IMAGE_START, &payload)
}

fn encode_clipboard_image_chunk(chunk: ClipboardImageChunk) -> Vec<u8> {
    assert!(!chunk.bytes.is_empty());
    assert!(chunk.bytes.len() <= MAX_CLIPBOARD_IMAGE_CHUNK_BYTES);
    let mut payload = Vec::with_capacity(16 + chunk.bytes.len());
    payload.extend_from_slice(&chunk.transfer_id.to_be_bytes());
    payload.extend_from_slice(&chunk.offset.to_be_bytes());
    payload.extend_from_slice(&chunk.bytes);
    encode(TAG_CLIPBOARD_IMAGE_CHUNK, &payload)
}

fn encode_clipboard_image_end(end: ClipboardImageEnd) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8 + FILE_EXPORT_DIGEST_BYTES);
    payload.extend_from_slice(&end.transfer_id.to_be_bytes());
    payload.extend_from_slice(&end.sha256);
    encode(TAG_CLIPBOARD_IMAGE_END, &payload)
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

fn write_capability_overrides(payload: &mut Vec<u8>, overrides: AttachCapabilityOverrides) {
    for value in [
        overrides.pointer_shapes,
        overrides.truecolor,
        overrides.synchronized_output,
        overrides.osc8_hyperlinks,
        overrides.underline_style,
    ] {
        payload.push(match value {
            None => 0,
            Some(false) => 1,
            Some(true) => 2,
        });
    }
    payload.push(match overrides.image_protocol {
        None => 0,
        Some(ImageProtocolCapability::Unsupported) => 1,
        Some(ImageProtocolCapability::Kitty) => 2,
    });
}

fn read_capability_overrides(cursor: &mut PayloadCursor<'_>) -> Result<AttachCapabilityOverrides> {
    if cursor.finished() {
        return Ok(AttachCapabilityOverrides::default());
    }
    let pointer_shapes = read_override_bool(cursor, "pointer shapes override")?;
    let truecolor = read_override_bool(cursor, "truecolor override")?;
    let synchronized_output = read_override_bool(cursor, "sync override")?;
    let osc8_hyperlinks = read_override_bool(cursor, "osc8 override")?;
    let underline_style = read_override_bool(cursor, "underline style override")?;
    let image_protocol = match cursor.read_u8("image protocol override")? {
        0 => None,
        1 => Some(ImageProtocolCapability::Unsupported),
        2 => Some(ImageProtocolCapability::Kitty),
        other => bail!("unknown image protocol override byte {other}"),
    };
    Ok(AttachCapabilityOverrides {
        pointer_shapes,
        truecolor,
        synchronized_output,
        osc8_hyperlinks,
        underline_style,
        image_protocol,
    })
}

fn read_override_bool(cursor: &mut PayloadCursor<'_>, label: &str) -> Result<Option<bool>> {
    match cursor.read_u8(label)? {
        0 => Ok(None),
        1 => Ok(Some(false)),
        2 => Ok(Some(true)),
        other => bail!("unknown {label} byte {other}"),
    }
}

async fn read_framed_payload<R>(stream: &mut R, first_byte: u8) -> Result<Option<(u8, Vec<u8>)>>
where
    R: AsyncRead + Unpin,
{
    let deadline = tokio::time::Instant::now() + FRAME_READ_TIMEOUT;
    let mut len_buf = [0u8; 4];
    match tokio::time::timeout_at(deadline, stream.read_exact(&mut len_buf)).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                return Ok(None);
            }
            return Err(e).context("attach frame: reading length prefix");
        }
        Err(_) => bail!("attach frame: timed out reading length prefix"),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let max_payload = max_frame_payload_for_tag(first_byte);
    if len > max_payload {
        bail!("attach frame payload {len} exceeds limit {max_payload}");
    }
    let mut payload: Vec<u8> = Vec::with_capacity(len.min(64 * 1024));
    let mut remaining = len;
    let mut chunk = [0u8; 16 * 1024];
    while remaining > 0 {
        let n = chunk.len().min(remaining);
        let Some(dest) = chunk.get_mut(..n) else {
            bail!("attach frame: chunk slice out of range");
        };
        match tokio::time::timeout_at(deadline, stream.read_exact(dest)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    return Ok(None);
                }
                return Err(e).context("attach frame: reading payload");
            }
            Err(_) => bail!("attach frame: timed out reading payload"),
        }
        let Some(filled) = chunk.get(..n) else {
            bail!("attach frame: chunk slice out of range");
        };
        payload.extend_from_slice(filled);
        remaining -= n;
    }
    Ok(Some((first_byte, payload)))
}

fn max_frame_payload_for_tag(tag: u8) -> usize {
    match tag {
        TAG_CLIPBOARD_IMAGE | TAG_ATTACH_CONTROL => MAX_CLIPBOARD_IMAGE_FRAME_PAYLOAD,
        TAG_CLIPBOARD_IMAGE_CHUNK => 16 + MAX_CLIPBOARD_IMAGE_CHUNK_BYTES,
        TAG_FILE_EXPORT_CHUNK => 16 + MAX_FILE_EXPORT_CHUNK_BYTES,
        _ => MAX_FRAME_PAYLOAD,
    }
}

#[expect(
    clippy::large_futures,
    reason = "attach frame reader decodes full ClientFrame/ServerFrame payloads; boxing every call site would add hot-path indirection"
)]
pub async fn read_client_frame<R>(stream: &mut R, first_byte: u8) -> Result<Option<ClientFrame>>
where
    R: AsyncRead + Unpin,
{
    let Some((tag, payload)) = read_framed_payload(stream, first_byte).await? else {
        return Ok(None);
    };
    Ok(Some(decode_client(tag, payload)?))
}

#[expect(
    clippy::large_futures,
    reason = "attach frame reader decodes full ClientFrame/ServerFrame payloads; boxing every call site would add hot-path indirection"
)]
pub async fn read_server_frame<R>(stream: &mut R, first_byte: u8) -> Result<Option<ServerFrame>>
where
    R: AsyncRead + Unpin,
{
    let Some((tag, payload)) = read_framed_payload(stream, first_byte).await? else {
        return Ok(None);
    };
    Ok(Some(decode_server(tag, payload)?))
}

pub fn decode_client(tag: u8, payload: Vec<u8>) -> Result<ClientFrame> {
    if tag == TAG_ATTACH_CONTROL {
        return contextual::decode_request(&payload).map(ClientFrame::AttachControl);
    }
    decode_client_legacy(tag, payload)
}

fn decode_client_legacy(tag: u8, payload: Vec<u8>) -> Result<ClientFrame> {
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
                        bail!("hello shell spawn must not carry an instance target");
                    }
                    Some(SpawnRequest::Shell)
                }
                2 => {
                    if agent.is_empty() {
                        bail!("hello instance spawn missing target");
                    }
                    Some(SpawnRequest::Instance(agent))
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
                capability_overrides: read_capability_overrides(&mut cursor)?,
            };
            let context_len = cursor.read_u16("telemetry context length")? as usize;
            let context: Option<TelemetryContext> =
                serde_json::from_slice(cursor.read_bytes(context_len, "telemetry context")?)
                    .context("decoding Hello telemetry context")?;
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
                context: context.map(Box::new),
            }
        }
        TAG_RESIZE => {
            let rows: [u8; 2] = payload
                .get(..2)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| anyhow::anyhow!("resize payload too short"))?;
            let cols: [u8; 2] = payload
                .get(2..4)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| anyhow::anyhow!("resize payload too short"))?;
            ClientFrame::Resize {
                rows: u16::from_be_bytes(rows),
                cols: u16::from_be_bytes(cols),
            }
        }
        TAG_INPUT => ClientFrame::Input(payload),
        TAG_COMMAND => ClientFrame::Command(payload),
        TAG_DETACH => ClientFrame::Detach,
        TAG_FOCUS_IN => ClientFrame::FocusIn,
        TAG_FOCUS_OUT => ClientFrame::FocusOut,
        TAG_CLIPBOARD_IMAGE => {
            let Some((&format_tag, bytes)) = payload.split_first() else {
                bail!("clipboard image payload too short");
            };
            let format = ClipboardImageFormat::from_tag(format_tag)?;
            if bytes.is_empty() {
                bail!("clipboard image payload is empty");
            }
            if bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES {
                bail!(
                    "clipboard image payload {} exceeds cap {MAX_CLIPBOARD_IMAGE_BYTES}",
                    bytes.len()
                );
            }
            ClientFrame::ClipboardImage(ClipboardImage {
                format,
                bytes: bytes.to_vec(),
            })
        }
        TAG_CLIPBOARD_IMAGE_START => {
            let mut cursor = PayloadCursor::new(&payload);
            let transfer_id = cursor.read_u64("clipboard image transfer id")?;
            let format = ClipboardImageFormat::from_tag(cursor.read_u8("clipboard image format")?)?;
            let size = cursor.read_u64("clipboard image size")?;
            if size == 0 {
                bail!("clipboard image transfer size is empty");
            }
            if size > MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES_U64 {
                bail!(
                    "clipboard image transfer size {size} exceeds cap {MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES}"
                );
            }
            if !cursor.finished() {
                bail!("clipboard image start payload has trailing bytes");
            }
            ClientFrame::ClipboardImageStart(ClipboardImageStart {
                transfer_id,
                format,
                size,
            })
        }
        TAG_CLIPBOARD_IMAGE_CHUNK => {
            let mut cursor = PayloadCursor::new(&payload);
            let transfer_id = cursor.read_u64("clipboard image transfer id")?;
            let offset = cursor.read_u64("clipboard image offset")?;
            let bytes = cursor
                .read_remaining("clipboard image chunk bytes")?
                .to_vec();
            if bytes.is_empty() {
                bail!("clipboard image chunk is empty");
            }
            if bytes.len() > MAX_CLIPBOARD_IMAGE_CHUNK_BYTES {
                bail!(
                    "clipboard image chunk length {} exceeds cap {MAX_CLIPBOARD_IMAGE_CHUNK_BYTES}",
                    bytes.len()
                );
            }
            ClientFrame::ClipboardImageChunk(ClipboardImageChunk {
                transfer_id,
                offset,
                bytes,
            })
        }
        TAG_CLIPBOARD_IMAGE_END => {
            let mut cursor = PayloadCursor::new(&payload);
            let transfer_id = cursor.read_u64("clipboard image transfer id")?;
            let digest = cursor.read_bytes(FILE_EXPORT_DIGEST_BYTES, "clipboard image sha256")?;
            if !cursor.finished() {
                bail!("clipboard image end payload has trailing bytes");
            }
            let sha256 = digest
                .try_into()
                .map_err(|_| anyhow::anyhow!("clipboard image sha256 slice length mismatch"))?;
            ClientFrame::ClipboardImageEnd(ClipboardImageEnd {
                transfer_id,
                sha256,
            })
        }
        TAG_CLIPBOARD_IMAGE_ERROR => {
            if payload.is_empty() {
                bail!("clipboard image error message is empty");
            }
            if payload.len() > MAX_CLIPBOARD_IMAGE_ERROR_BYTES {
                bail!(
                    "clipboard image error message length {} exceeds cap {MAX_CLIPBOARD_IMAGE_ERROR_BYTES}",
                    payload.len()
                );
            }
            let message = std::str::from_utf8(&payload)
                .context("clipboard image error message is not valid UTF-8")?;
            ClientFrame::ClipboardImageError(ClipboardImageError::from_message(message.to_owned()))
        }
        TAG_HOST_NOTICE => {
            if payload.is_empty() {
                bail!("host notice message is empty");
            }
            if payload.len() > MAX_HOST_NOTICE_BYTES {
                bail!(
                    "host notice message length {} exceeds cap {MAX_HOST_NOTICE_BYTES}",
                    payload.len()
                );
            }
            let message =
                std::str::from_utf8(&payload).context("host notice message is not valid UTF-8")?;
            ClientFrame::HostNotice(message.to_owned())
        }
        other => bail!("unknown client attach tag {other:#04x}"),
    })
}

pub fn decode_server(tag: u8, payload: Vec<u8>) -> Result<ServerFrame> {
    Ok(match tag {
        TAG_WELCOME => {
            let count: [u8; 4] = payload
                .get(..4)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| anyhow::anyhow!("welcome payload too short"))?;
            ServerFrame::Welcome {
                session_count: u32::from_be_bytes(count),
            }
        }
        TAG_OUTPUT => ServerFrame::Output(payload),
        TAG_SESSION_LIST => ServerFrame::SessionList(payload),
        TAG_SHUTDOWN => {
            let reason = if payload.is_empty() {
                None
            } else {
                Some(
                    String::from_utf8(payload)
                        .context("shutdown reason payload is not valid UTF-8")?,
                )
            };
            ServerFrame::Shutdown { reason }
        }
        TAG_BELL => ServerFrame::Bell,
        TAG_HOST_OPEN_URL => {
            let url = std::str::from_utf8(&payload)
                .map_err(|_| anyhow::anyhow!("host-open-url payload is not valid UTF-8"))?;
            if !is_host_open_url_scheme(url) {
                bail!("host-open-url payload must use an allowlisted scheme");
            }
            ServerFrame::HostOpenUrl(url.to_owned())
        }
        TAG_FILE_EXPORT_START => {
            let mut cursor = PayloadCursor::new(&payload);
            let transfer_id = cursor.read_u64("file export transfer id")?;
            let size = cursor.read_u64("file export size")?;
            let source_len = cursor.read_u16("file export source path length")? as usize;
            let name_len = cursor.read_u16("file export file name length")? as usize;
            let reveal_after_export = match cursor.read_u8("file export reveal flag")? {
                0 => false,
                1 => true,
                other => bail!("file export reveal flag must be 0 or 1, got {other}"),
            };
            let open_after_export = match cursor.read_u8("file export open flag")? {
                0 => false,
                1 => true,
                other => bail!("file export open flag must be 0 or 1, got {other}"),
            };
            if source_len == 0 || source_len > MAX_FILE_EXPORT_PATH_BYTES {
                bail!(
                    "file export source path length {source_len} exceeds cap {MAX_FILE_EXPORT_PATH_BYTES}"
                );
            }
            if name_len == 0 || name_len > MAX_FILE_EXPORT_NAME_BYTES {
                bail!(
                    "file export file name length {name_len} exceeds cap {MAX_FILE_EXPORT_NAME_BYTES}"
                );
            }
            let source_path = cursor.read_string(source_len, "file export source path")?;
            let file_name = cursor.read_string(name_len, "file export file name")?;
            if !cursor.finished() {
                bail!("file export start payload has trailing bytes");
            }
            ServerFrame::FileExportStart(FileExportStart {
                transfer_id,
                source_path,
                file_name,
                size,
                reveal_after_export,
                open_after_export,
            })
        }
        TAG_FILE_EXPORT_CHUNK => {
            let mut cursor = PayloadCursor::new(&payload);
            let transfer_id = cursor.read_u64("file export transfer id")?;
            let offset = cursor.read_u64("file export offset")?;
            let bytes = cursor.read_remaining("file export chunk bytes")?.to_vec();
            if bytes.is_empty() {
                bail!("file export chunk is empty");
            }
            if bytes.len() > MAX_FILE_EXPORT_CHUNK_BYTES {
                bail!(
                    "file export chunk length {} exceeds cap {MAX_FILE_EXPORT_CHUNK_BYTES}",
                    bytes.len()
                );
            }
            ServerFrame::FileExportChunk(FileExportChunk {
                transfer_id,
                offset,
                bytes,
            })
        }
        TAG_FILE_EXPORT_END => {
            let mut cursor = PayloadCursor::new(&payload);
            let transfer_id = cursor.read_u64("file export transfer id")?;
            let digest = cursor.read_bytes(FILE_EXPORT_DIGEST_BYTES, "file export sha256")?;
            if !cursor.finished() {
                bail!("file export end payload has trailing bytes");
            }
            let sha256 = digest
                .try_into()
                .map_err(|_| anyhow::anyhow!("file export sha256 slice length mismatch"))?;
            ServerFrame::FileExportEnd(FileExportEnd {
                transfer_id,
                sha256,
            })
        }
        TAG_HOST_REVEAL_PATH => {
            if payload.is_empty() {
                bail!("host reveal path payload is empty");
            }
            if payload.len() > MAX_HOST_REVEAL_PATH_BYTES {
                bail!(
                    "host reveal path length {} exceeds cap {MAX_HOST_REVEAL_PATH_BYTES}",
                    payload.len()
                );
            }
            let path = std::str::from_utf8(&payload)
                .map_err(|_| anyhow::anyhow!("host reveal path payload is not valid UTF-8"))?;
            ServerFrame::HostRevealPath(path.to_owned())
        }
        TAG_HOST_STAGE_IMAGE_FROM_CLIPBOARD_PATH => {
            if !payload.is_empty() {
                bail!("host stage image path request payload must be empty");
            }
            ServerFrame::HostStageImageFromClipboardPath
        }
        TAG_HOST_PASTE_IMAGE_FROM_CLIPBOARD => {
            if !payload.is_empty() {
                bail!("host paste image request payload must be empty");
            }
            ServerFrame::HostPasteImageFromClipboard
        }
        TAG_HOST_STAGE_IMAGE_FROM_CLIPBOARD => {
            if !payload.is_empty() {
                bail!("host stage image request payload must be empty");
            }
            ServerFrame::HostStageImageFromClipboard
        }
        TAG_ATTACH_CONTROL_RESPONSE => {
            ServerFrame::AttachControlResponse(contextual::decode_response(&payload)?)
        }
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
        let Some(value) = self.payload.get(self.pos).copied() else {
            bail!("hello payload ended before {field}");
        };
        self.pos += 1;
        Ok(value)
    }

    fn read_u16(&mut self, field: &str) -> Result<u16> {
        let bytes = self.read_bytes(2, field)?;
        let arr: [u8; 2] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("hello {field}: short slice"))?;
        Ok(u16::from_be_bytes(arr))
    }

    fn read_u32(&mut self, field: &str) -> Result<u32> {
        let bytes = self.read_bytes(4, field)?;
        let arr: [u8; 4] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("hello {field}: short slice"))?;
        Ok(u32::from_be_bytes(arr))
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

    fn read_remaining(&mut self, field: &str) -> Result<&'a [u8]> {
        let Some(bytes) = self.payload.get(self.pos..) else {
            bail!("hello payload ended before {field}");
        };
        self.pos = self.payload.len();
        Ok(bytes)
    }

    fn read_bytes(&mut self, len: usize, field: &str) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| anyhow::anyhow!("hello {field} length overflow"))?;
        let Some(bytes) = self.payload.get(self.pos..end) else {
            bail!(
                "hello {field} length {len} exceeds remaining payload {}",
                self.payload.len().saturating_sub(self.pos)
            );
        };
        self.pos = end;
        Ok(bytes)
    }
}

fn is_host_open_url_scheme(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}
