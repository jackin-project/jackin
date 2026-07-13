// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Typed TUI-side requests for non-visual work.
//!
//! The capsule daemon still owns PTY/session/control-plane authority outside
//! the TUI boundary. This enum is intentionally small until more daemon actions
//! are routed through update functions instead of direct daemon methods.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Ask the attach client to send raw bytes to the daemon socket.
    SendAttachInput(Vec<u8>),
    /// Ask the attach client to report a resized outer terminal.
    ResizeAttachClient { rows: u16, cols: u16 },
}

/// Stages of the takeover/first-attach burst. Each variant maps to a
/// human-readable label used in the clog line when the send fails so a
/// dropped initial frame is observable in the multiplexer log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitialFrameKind {
    Welcome,
    ClientOwnedModes,
    FirstAttach,
}

impl InitialFrameKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::ClientOwnedModes => "client-owned mode state",
            Self::FirstAttach => "first-attach frame",
        }
    }
}
