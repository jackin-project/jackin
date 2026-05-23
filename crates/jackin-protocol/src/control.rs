//! Control channel: length-prefixed JSON request / response messages.
//!
//! Used by the host CLI for one-shot queries — `status`, `snapshot`,
//! and future `session.create` / `session.kill` / `session.title` /
//! `events`. The host opens a Unix socket connection, writes one
//! framed JSON request, reads one framed JSON response, and
//! disconnects.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// Request the current session inventory.
    Status,
    /// Request the tab/pane tree snapshot.
    Snapshot,
    /// Forward-compat sink for variants added by a newer peer.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    /// Current session inventory.
    SessionList { sessions: Vec<SessionInfo> },
    /// Tab/pane tree snapshot. `tabs` is in render order;
    /// `active_tab` indexes into it. Each `TabSnapshot::panes` lists
    /// the pane leaves of that tab in `PaneTree` in-order traversal
    /// order; `TabSnapshot::focused_pane` carries the session id of
    /// the focused leaf (matches a `PaneSnapshot::session_id`).
    Snapshot {
        tabs: Vec<TabSnapshot>,
        active_tab: u32,
    },
    /// Forward-compat sink for variants added by a newer peer.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: u64,
    pub label: String,
    pub agent: Option<String>,
    pub state: AgentState,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub label: String,
    /// `session_id` of the focused leaf in this tab. Always matches
    /// one of the `panes[*].session_id` entries.
    pub focused_pane: u64,
    pub panes: Vec<PaneSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneSnapshot {
    pub session_id: u64,
    /// Session label (agent slug or "Shell").
    pub label: String,
    /// `None` for shell sessions; the agent slug otherwise.
    pub agent: Option<String>,
    pub state: AgentState,
}

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

/// Encode `msg` as a 4-byte big-endian length prefix + UTF-8 JSON body.
///
/// `to_vec` cannot actually fail for `ClientMsg` or `ServerMsg` — their
/// derived `Serialize` impls only emit JSON-representable variants — so
/// the panic doubles as a contract: any future variant that breaks the
/// invariant surfaces immediately in tests instead of silently shipping
/// a 4-byte length=0 frame the peer interprets as an empty payload.
///
/// `ServerMsg::Unknown` IS a legitimate reply (socket.rs returns it as
/// the response to an unknown `ClientMsg` so the peer's `read_exact`
/// returns immediately instead of hanging until `SOCKET_TIMEOUT`), so
/// the encode side intentionally serializes it as `{"type":"unknown"}`.
/// Peers re-decode it as `Unknown` and the host CLI surfaces the
/// mismatch as an operator-facing error.
pub fn frame(msg: &impl Serialize) -> Vec<u8> {
    let json =
        serde_json::to_vec(msg).expect("control-channel message serialization is infallible");
    let len = (json.len() as u32).to_be_bytes();
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len);
    out.extend_from_slice(&json);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_msg_unknown_decodes_from_unrecognised_tag() {
        let m: ClientMsg = serde_json::from_str(r#"{"type":"future_query"}"#)
            .expect("decode unknown ClientMsg variant");
        assert!(matches!(m, ClientMsg::Unknown));
    }

    #[test]
    fn server_msg_unknown_decodes_from_unrecognised_tag() {
        let m: ServerMsg = serde_json::from_str(r#"{"type":"future_reply"}"#)
            .expect("decode unknown ServerMsg variant");
        assert!(matches!(m, ServerMsg::Unknown));
    }

    #[test]
    fn missing_tag_field_still_bails() {
        // Structural malformations (no `type` key, non-string tag) are
        // not absorbed by `#[serde(other)]` — peers must still emit
        // well-formed tagged JSON.
        assert!(serde_json::from_str::<ClientMsg>(r#"{"foo":"bar"}"#).is_err());
        assert!(serde_json::from_str::<ServerMsg>(r#"{"type":42}"#).is_err());
    }

    #[test]
    fn known_variants_roundtrip() {
        let json = serde_json::to_string(&ClientMsg::Status).unwrap();
        assert_eq!(json, r#"{"type":"status"}"#);
        let decoded: ClientMsg = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, ClientMsg::Status));
    }
}
