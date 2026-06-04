//! Typed passthrough event stream — supersedes `vt100::Callbacks` + `OscCapture`.
//!
//! Phase 2 v0: collect events in a `Vec<PassthroughEvent>` for the caller to
//! drain.  Phase 5 promotes this to a typed async stream.

/// A typed passthrough event produced by escape sequences the capsule must
/// handle outside the grid (OSC, mode changes, scrollback control).
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
    /// CSI `?2026h` / `?2026l`: synchronized output enable/disable.
    SynchronizedOutput(bool),
    /// Unhandled CSI — forwarded raw for passthrough.
    UnhandledCsi(Vec<u8>),
    /// Capsule-specific: clear the scrollback buffer.
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
