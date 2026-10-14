// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::input::{InputBindings, parse_key_binding};

#[must_use]
pub fn resolve_input_bindings() -> InputBindings {
    InputBindings {
        prefix: resolve_prefix_binding(),
        palette_key: resolve_palette_binding(),
    }
}

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
