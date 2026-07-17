// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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

/// Prefix mode is opt-in: returns `Some(byte)` when `JACKIN_PREFIX`
/// is set to a parseable key, `None` otherwise.
fn resolve_prefix_binding() -> Option<u8> {
    let raw = std::env::var("JACKIN_PREFIX").ok();
    prefix_binding(raw.as_deref())
}

fn prefix_binding(raw: Option<&str>) -> Option<u8> {
    let s = raw?;
    if s.eq_ignore_ascii_case("none") {
        return None;
    }
    parse_key_binding(s)
}

/// Palette key defaults to `Ctrl+\` (`0x1C`). Set
/// `JACKIN_PALETTE_KEY=none` to disable the direct-palette shortcut.
fn resolve_palette_binding() -> Option<u8> {
    let raw = std::env::var("JACKIN_PALETTE_KEY").ok();
    palette_binding(raw.as_deref())
}

fn palette_binding(raw: Option<&str>) -> Option<u8> {
    match raw {
        None => Some(0x1C),
        Some(s) if s.eq_ignore_ascii_case("none") => None,
        Some(s) => {
            if let Some(byte) = parse_key_binding(s) {
                Some(byte)
            } else {
                Some(0x1C)
            }
        }
    }
}

#[cfg(test)]
mod tests;
