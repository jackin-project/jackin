//! Keyboard binding resolution: map raw key sequences to capsule actions
//! (focus switch, new tab, close, palette open, etc.).
//!
//! Not responsible for: dispatching resolved actions or reading PTY input
//! bytes at runtime — that is `tui::input`.
//!
//! Key invariant: bindings are resolved once at startup from environment
//! variables (`JACKIN_PREFIX`, `JACKIN_PALETTE_KEY`); the resolved
//! `InputBindings` is immutable for the lifetime of the session.

use crate::tui::input::{InputBindings, parse_key_binding};

pub fn resolve_input_bindings() -> InputBindings {
    InputBindings {
        prefix: resolve_prefix_binding(),
        palette_key: resolve_palette_binding(),
    }
}

/// Short display label for a raw key byte, suitable for status-bar legend.
/// Control codes render as `C-x`; printable ASCII as the character itself.
pub fn palette_key_glyph(key: Option<u8>) -> Option<String> {
    let b = key?;
    Some(match b {
        0x01..=0x1a => format!("C-{}", (b'a' + (b - 1)) as char),
        0x1b => "Esc".to_owned(),
        0x1c => "C-\\".to_owned(),
        0x1d => "C-]".to_owned(),
        0x1e => "C-^".to_owned(),
        0x1f => "C-_".to_owned(),
        0x7f => "Del".to_owned(),
        b if b.is_ascii_graphic() => String::from(b as char),
        _ => format!("0x{b:02x}"),
    })
}

/// Prefix mode is opt-in: returns `Some(byte)` when `JACKIN_PREFIX`
/// is set to a parseable key, `None` otherwise.
fn resolve_prefix_binding() -> Option<u8> {
    let s = std::env::var("JACKIN_PREFIX").ok()?;
    if s.eq_ignore_ascii_case("none") {
        return None;
    }
    if let Some(byte) = parse_key_binding(&s) {
        Some(byte)
    } else {
        crate::clog!("invalid JACKIN_PREFIX={s:?}; prefix mode disabled");
        None
    }
}

/// Palette key defaults to `Ctrl+\` (`0x1C`). Set
/// `JACKIN_PALETTE_KEY=none` to disable the direct-palette shortcut.
fn resolve_palette_binding() -> Option<u8> {
    match std::env::var("JACKIN_PALETTE_KEY") {
        Err(_) => Some(0x1C),
        Ok(s) if s.eq_ignore_ascii_case("none") => None,
        Ok(s) => {
            if let Some(byte) = parse_key_binding(&s) {
                Some(byte)
            } else {
                crate::clog!("invalid JACKIN_PALETTE_KEY={s:?}; using default Ctrl+\\");
                Some(0x1C)
            }
        }
    }
}
