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
