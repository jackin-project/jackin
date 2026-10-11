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
