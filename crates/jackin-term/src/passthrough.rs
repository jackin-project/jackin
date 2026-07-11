//! Typed passthrough event stream for OSC/CSI events the grid should not render.
//!
//! Phase 2 v0: collect events in a `Vec<PassthroughEvent>` for the caller to
//! drain.  Phase 5 promotes this to a typed async stream and wires it into
//! `session.rs` consumes these events after feeding PTY bytes to `DamageGrid`.

/// A typed passthrough event produced by escape sequences the capsule must
/// handle outside the grid (OSC, mode changes, scrollback control, focus).
///
/// These events are the typed output side of the PTY byte parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassthroughEvent {
    /// OSC 0 / OSC 2: window title change.
    TitleChanged(String),
    /// OSC 1: window icon name change.
    IconNameChanged(String),
    /// OSC 52: clipboard write. Carries the full payload after the OSC code —
    /// `<selection>;<base64>` (e.g. `c;SGVsbG8=`) — so `encode` can reproduce
    /// the exact `\x1b]52;<payload>\x07` the program emitted.
    ClipboardWrite(String),
    /// OSC 7: current working directory (URI).
    CwdChanged(String),
    /// OSC 9 / OSC 99: desktop notification.
    Notification(String),
    /// CSI `?1h` / `?1l`: application cursor keys mode.
    ApplicationCursorKeys(bool),
    /// CSI `?1004h` / `?1004l`: focus events enable/disable.
    FocusEvents(bool),
    /// CSI `?2004h` / `?2004l`: bracketed paste enable/disable.
    BracketedPaste(bool),
    /// OSC 8: clickable hyperlink (id, uri).
    ///
    /// `id` is the link id (empty = anonymous), `uri` is the target URI.
    /// An empty `uri` ends the hyperlink (equivalent to OSC 8;;ST).
    /// The capsule applies a URI-scheme safety filter before forwarding.
    Hyperlink { id: String, uri: String },
    /// Allowlisted unhandled CSI — forwarded raw for passthrough. Only the
    /// documented allowlist reaches the client this way (kitty keyboard
    /// push/pop, xterm modifyOtherKeys); everything else becomes
    /// [`PassthroughEvent::DroppedCsi`].
    UnhandledCsi(Vec<u8>),
    /// Default-denied CSI: not handled by the grid and not on the forward
    /// allowlist. Carried out so the capsule can debug-log the exact bytes;
    /// never encoded for the client.
    DroppedCsi(Vec<u8>),
    /// Reply to a device/mode query (DA, DSR, DECRQM, kitty-keyboard query)
    /// the emulator answered itself. The bytes go back to the AGENT's PTY
    /// stdin, never to the outer terminal — the agent queried the capsule's
    /// emulator, not the host. Forwarding such queries to the host let the
    /// host advertise capabilities (grapheme-width mode 2027, kitty keyboard,
    /// …) the grid does not emulate, which desynced the agent's column math
    /// from the grid and corrupted alt-screen rendering.
    Reply(Vec<u8>),
    /// Capsule-specific: clear the scrollback buffer (CSI 3J).
    ScrollbackClear,
}

impl PassthroughEvent {
    /// Encode the event as a raw ANSI/OSC byte sequence for forwarding to an
    /// outer terminal.
    ///
    /// This is the encode half of the typed-passthrough model: the capsule
    /// can convert `PassthroughEvent`s back to bytes and forward them to the
    /// operator's outer terminal, replacing the raw-byte pass-through used by
    /// `OscCapture`.
    ///
    /// `ScrollbackClear` has no outer-terminal representation (it is an
    /// internal capsule instruction) and returns `None`.
    #[must_use]
    pub fn encode(&self) -> Option<Vec<u8>> {
        match self {
            // OSC sequences — use BEL terminator (ST `\x07` is widely supported).
            Self::TitleChanged(title) => Some(format!("\x1b]0;{title}\x07").into_bytes()),
            Self::IconNameChanged(name) => Some(format!("\x1b]1;{name}\x07").into_bytes()),
            Self::ClipboardWrite(payload) => Some(format!("\x1b]52;{payload}\x07").into_bytes()),
            Self::CwdChanged(uri) => Some(format!("\x1b]7;{uri}\x07").into_bytes()),
            Self::Notification(msg) => Some(format!("\x1b]9;{msg}\x07").into_bytes()),
            // DEC private mode toggles.
            Self::ApplicationCursorKeys(on) => Some(if *on {
                b"\x1b[?1h".to_vec()
            } else {
                b"\x1b[?1l".to_vec()
            }),
            Self::FocusEvents(on) => Some(if *on {
                b"\x1b[?1004h".to_vec()
            } else {
                b"\x1b[?1004l".to_vec()
            }),
            Self::BracketedPaste(on) => Some(if *on {
                b"\x1b[?2004h".to_vec()
            } else {
                b"\x1b[?2004l".to_vec()
            }),
            // OSC 8 hyperlink — emit if uri is non-empty (else close hyperlink).
            Self::Hyperlink { id, uri } => {
                if uri.is_empty() {
                    // Close hyperlink: OSC 8 ; ; ST
                    Some(b"\x1b]8;;\x07".to_vec())
                } else {
                    // Open hyperlink: OSC 8 ; id ; uri ST
                    Some(format!("\x1b]8;{id};{uri}\x07").into_bytes())
                }
            }
            // Raw pass-through — emit as-is.
            Self::UnhandledCsi(bytes) => Some(bytes.clone()),
            // No outer-terminal output: dropped CSI, PTY-only replies, capsule-internal.
            Self::DroppedCsi(_) | Self::Reply(_) | Self::ScrollbackClear => None,
        }
    }
}

/// Collects `PassthroughEvent`s during a `process()` call.
#[derive(Debug, Default)]
pub struct PassthroughBuffer {
    events: Vec<PassthroughEvent>,
}

impl PassthroughBuffer {
    /// Drain and return all buffered events.
    pub fn drain(&mut self) -> Vec<PassthroughEvent> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn push(&mut self, event: PassthroughEvent) {
        self.events.push(event);
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}
