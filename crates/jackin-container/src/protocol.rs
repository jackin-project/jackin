use serde::{Deserialize, Serialize};

/// Messages sent from client → daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Initial handshake: client announces its terminal size.
    Hello { rows: u16, cols: u16 },
    /// Raw input bytes from the client terminal (base64-encoded).
    Input { data: String },
    /// Client terminal was resized.
    Resize { rows: u16, cols: u16 },
    /// Create a new agent or shell session.
    NewSession { agent: Option<String> },
    /// Switch active session to the given ID.
    SwitchSession { id: u64 },
    /// Kill a session by ID.
    KillSession { id: u64 },
    /// Request current session list.
    Status,
}

/// Messages sent from daemon → client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Welcome sent immediately after Hello, confirming connection.
    Welcome { session_count: usize },
    /// Raw terminal output to display (base64-encoded).
    Output { data: String },
    /// Redraw the full screen from the daemon's compositor.
    Redraw { rows: u16, cols: u16, screen: Vec<String> },
    /// Current session inventory (response to Status or on change).
    SessionList { sessions: Vec<SessionInfo> },
    /// Daemon is shutting down.
    Shutdown,
}

/// Summary of a single session, sent in SessionList messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u64,
    pub label: String,
    pub agent: Option<String>,
    pub state: AgentState,
    pub active: bool,
}

/// Four-state agent status, inspired by Herdr's design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Working,
    Blocked,
    Done,
    Idle,
}

impl AgentState {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Idle => "idle",
        }
    }
}

/// Encode bytes as base64 for JSON transport.
pub fn b64_encode(data: &[u8]) -> String {
    use std::fmt::Write as _;
    // Minimal base64 without pulling in a dependency — the alphabet is fixed.
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        let _ = write!(out, "{}{}{}{}",
            ALPHABET[((n >> 18) & 0x3f) as usize] as char,
            ALPHABET[((n >> 12) & 0x3f) as usize] as char,
            if chunk.len() > 1 { ALPHABET[((n >> 6) & 0x3f) as usize] as char } else { '=' },
            if chunk.len() > 2 { ALPHABET[(n & 0x3f) as usize] as char } else { '=' },
        );
    }
    out
}

/// Decode base64 from JSON transport.
pub fn b64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u8 {
        match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    };
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let b0 = val(chunk[0]);
        let b1 = val(chunk.get(1).copied().unwrap_or(0));
        let b2 = val(chunk.get(2).copied().unwrap_or(0));
        let b3 = val(chunk.get(3).copied().unwrap_or(0));
        out.push((b0 << 2) | (b1 >> 4));
        if chunk.len() > 2 { out.push((b1 << 4) | (b2 >> 2)); }
        if chunk.len() > 3 { out.push((b2 << 6) | b3); }
    }
    out
}

/// Framed message: 4-byte big-endian length prefix + UTF-8 JSON body.
pub fn frame(msg: &impl Serialize) -> Vec<u8> {
    let json = serde_json::to_vec(msg).unwrap_or_default();
    let len = (json.len() as u32).to_be_bytes();
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(&json);
    out
}
