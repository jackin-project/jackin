// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Capsule TUI input parsing: classify raw terminal bytes into palette/prefix
//! key events, mouse events, and PTY pass-through sequences.
//!
//! Not responsible for: acting on classified events (see `daemon` dispatch) or
//! rendering (see `tui` render modules).

/// Input from the attached client terminal.
///
/// Two parallel models are supported:
///
/// - **Palette key (default `Ctrl+\`)** — one keystroke opens the
///   command palette and the operator picks an action from a list,
///   launcher-style. This is the primary UX and the only model the
///   default status-bar hint advertises. `Ctrl+\` is the byte `0x1C`
///   — no agent uses it as an editing key, it never appears in agent
///   output, and raw-mode terminals never emit it as content (the
///   `SIGQUIT` semantic only applies in cooked mode).
///
/// - **Prefix key (opt-in via `JACKIN_PREFIX=C-b`)** — tmux-style
///   prefix + command-key for operators who prefer direct keyboard
///   navigation. Disabled by default.
///
/// Both models can run simultaneously when both env vars are set.
/// `JACKIN_PALETTE_KEY=none` disables the palette key entirely.
/// `JACKIN_PALETTE_KEY=C-j` binds the palette to `Ctrl+J`, which is
/// the same byte multi-line agents and shells use as line-continuation
/// — so the bind collides with editing in those programs; set only
/// when the trade-off is acceptable.
/// A second click on the active tab cell within this window is a
/// TUI double-click and opens the rename-tab dialog.
pub(crate) const TAB_DOUBLE_CLICK_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(500);

/// `JACKIN_ESCAPE_TIME` env var — operator-tunable in milliseconds.
pub(crate) const ENV_ESCAPE_TIME: &str = "JACKIN_ESCAPE_TIME";

/// 50 ms matches tmux's default. Below human perception while
/// surviving slow ssh / paste chunks.
pub(crate) const DEFAULT_ESCAPE_TIME: std::time::Duration = std::time::Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputBindings {
    pub prefix: Option<u8>,
    pub palette_key: Option<u8>,
}

impl Default for InputBindings {
    fn default() -> Self {
        Self {
            prefix: None,
            palette_key: Some(0x1C),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Data(Vec<u8>),
    MousePress {
        col: u16,
        row: u16,
        button: u8,
    },
    /// SGR mouse release (`\x1b[< ... m`). Carries the same fields as
    /// `MousePress` so the daemon can drop both press and release on
    /// the same gate: shells and pre-mount agents that never enabled
    /// any mouse protocol must not see the raw SGR bytes as input.
    MouseRelease {
        col: u16,
        row: u16,
        button: u8,
    },
    PrefixCommand(PrefixCommand),
    /// Direct one-key shortcut → open the palette dialog. Distinct from
    /// `PrefixCommand::Palette`, which fires only after the prefix
    /// gesture; the daemon collapses both into the same dialog open.
    OpenPalette,
    /// `Ctrl+Q` (byte `0x11`) → open the "Exit jackin❯?" confirmation. The
    /// quit chord is consistent with every other jackin❯ surface; the dialog
    /// warns that exiting force-stops the container before it does so.
    RequestExit,
    /// Resize the focused pane in `dir` by one step. Emitted by
    /// `Alt-Shift-Arrow` so the operator can drag a split without
    /// reaching for the mouse. Steps are ratio-based (~5%) so the
    /// gesture is independent of terminal size.
    ResizePane(ArrowDir),
    FocusIn,
    FocusOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixCommand {
    NewTab,
    NextTab,
    PrevTab,
    JumpTab(usize),
    SplitTopBottom,
    SplitSideBySide,
    MoveFocus(ArrowDir),
    ZoomToggle,
    KillPane,
    KillTab,
    ClearPane,
    Detach,
    Usage,
    Palette,
    Redraw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDir {
    Left,
    Right,
    Up,
    Down,
}

#[must_use]
pub fn parse_prefix(s: &str) -> Option<u8> {
    parse_key_binding(s)
}

/// Accept:
/// - `C-a` ... `C-z` (case-insensitive) - `Ctrl+letter`, maps to `0x01..=0x1A`
/// - `C-\` / `C-]` / `C-^` / `C-_` - `Ctrl+symbol`, maps to `0x1C..=0x1F`
/// - `C-Space` or `C-@` - `Ctrl+Space` / `Ctrl+@`, maps to `0x00`
/// - A single ASCII control byte in hex form `0xNN`
/// - A single literal byte
#[must_use]
pub fn parse_key_binding(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("C-").or_else(|| s.strip_prefix("c-")) {
        if rest.eq_ignore_ascii_case("space") || rest == "@" {
            return Some(0x00);
        }
        let c = rest.chars().next()?;
        if c.is_ascii_alphabetic() {
            let upper = c.to_ascii_uppercase() as u8;
            return Some(upper - b'A' + 1);
        }
        return match c {
            '\\' => Some(0x1C),
            ']' => Some(0x1D),
            '^' => Some(0x1E),
            '_' => Some(0x1F),
            _ => None,
        };
    }
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u8::from_str_radix(hex, 16).ok();
    }
    if s.len() == 1 {
        return Some(s.as_bytes()[0]);
    }
    None
}

fn prefix_binding(b: u8) -> Option<PrefixCommand> {
    use crate::tui::keymap::raw_bytes_to_chord;
    let chord = raw_bytes_to_chord(&[b])?;
    crate::tui::keymap::PREFIX_COMMAND_KEYMAP.dispatch(chord)
}

fn parse_csi_u_key(rest: &[u8]) -> Option<(u32, Option<u32>, Option<u32>)> {
    let mut parts = rest.splitn(2, |&b| b == b';');
    let codepoint = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    let Some(modifier_and_event) = parts.next() else {
        return Some((codepoint, None, None));
    };
    let mut modifier_parts = modifier_and_event.splitn(2, |&b| b == b':');
    let modifier = std::str::from_utf8(modifier_parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    let event = modifier_parts
        .next()
        .and_then(|raw| std::str::from_utf8(raw).ok())
        .and_then(|raw| raw.parse::<u32>().ok());
    Some((codepoint, Some(modifier), event))
}

fn parse_xterm_modify_other_keys(seq: &[u8]) -> Option<(u32, u32)> {
    let body = seq.strip_prefix(b"\x1b[")?.strip_suffix(b"~")?;
    let mut parts = body.split(|&b| b == b';');
    let prefix = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    if prefix != 27 {
        return None;
    }
    let modifier = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    let codepoint = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<u32>()
        .ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((codepoint, modifier))
}

mod classify;
mod mouse;
mod parser;
#[cfg(test)]
pub(crate) use classify::csi_u_control_byte;
pub(crate) use classify::{CsiClassification, classify_csi, classify_x10_mouse};
#[cfg(test)]
pub(crate) use mouse::mouse_event_allowed_for_mode;
pub(crate) use mouse::{
    SGR_NO_BUTTON_MOTION, encode_mouse_for_protocol, encode_wheel_cursor_fallback, is_wheel_button,
    mouse_event_encoding_for_mode, pane_wheel_cursor_fallback_reason,
};
pub use parser::InputParser;
#[cfg(test)]
pub(crate) use parser::MAX_ESC_SEQ_LEN;

#[cfg(test)]
mod tests;
