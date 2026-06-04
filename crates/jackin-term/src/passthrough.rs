//! Typed passthrough event stream — supersedes `vt100::Callbacks` + `OscCapture`.
//!
//! Phase 2 v0: collect events in a `Vec<PassthroughEvent>` for the caller to
//! drain.  Phase 5 promotes this to a typed async stream and wires it into
//! `session.rs` to replace the `vt100::Callbacks + OscCapture` pattern.

/// A typed passthrough event produced by escape sequences the capsule must
/// handle outside the grid (OSC, mode changes, scrollback control, focus).
///
/// These events supersede the `vt100::Callbacks` trait methods in Phase 5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassthroughEvent {
    /// OSC 0 / OSC 2: window title change.
    TitleChanged(String),
    /// OSC 1: window icon name change.
    IconNameChanged(String),
    /// OSC 52: clipboard write (base64-encoded content).
    ClipboardWrite(String),
    /// OSC 7: current working directory (URI).
    CwdChanged(String),
    /// OSC 9 / OSC 99: desktop notification.
    Notification(String),
    /// CSI `?2026h` / `?2026l`: synchronized output enable/disable.
    SynchronizedOutput(bool),
    /// CSI `?1h` / `?1l`: application cursor keys mode.
    ApplicationCursorKeys(bool),
    /// CSI `?1004h` / `?1004l`: focus events enable/disable.
    FocusEvents(bool),
    /// CSI `?2004h` / `?2004l`: bracketed paste enable/disable.
    BracketedPaste(bool),
    /// Unhandled CSI — forwarded raw for passthrough.
    UnhandledCsi(Vec<u8>),
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
            Self::ClipboardWrite(b64) => Some(format!("\x1b]52;c;{b64}\x07").into_bytes()),
            Self::CwdChanged(uri) => Some(format!("\x1b]7;{uri}\x07").into_bytes()),
            Self::Notification(msg) => Some(format!("\x1b]9;{msg}\x07").into_bytes()),
            // DEC private mode toggles.
            Self::SynchronizedOutput(on) => Some(if *on {
                b"\x1b[?2026h".to_vec()
            } else {
                b"\x1b[?2026l".to_vec()
            }),
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
            // Raw pass-through — emit as-is.
            Self::UnhandledCsi(bytes) => Some(bytes.clone()),
            // Capsule-internal instruction; no outer-terminal output.
            Self::ScrollbackClear => None,
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
