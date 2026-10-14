// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(crate) const TAB_DOUBLE_CLICK_WINDOW: std::time::Duration =
    std::time::Duration::from_millis(500);

pub(crate) const ENV_ESCAPE_TIME: &str = "JACKIN_ESCAPE_TIME";

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
    MousePress { col: u16, row: u16, button: u8 },

    MouseRelease { col: u16, row: u16, button: u8 },
    PrefixCommand(PrefixCommand),

    OpenPalette,

    RequestExit,

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
