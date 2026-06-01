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
    let s = std::env::var("JACKIN_PREFIX").ok()?;
    if s.eq_ignore_ascii_case("none") {
        return None;
    }
    match parse_key_binding(&s) {
        Some(byte) => Some(byte),
        None => {
            crate::clog!("invalid JACKIN_PREFIX={s:?}; prefix mode disabled");
            None
        }
    }
}

/// Palette key defaults to `Ctrl+\` (`0x1C`). Set
/// `JACKIN_PALETTE_KEY=none` to disable the direct-palette shortcut.
fn resolve_palette_binding() -> Option<u8> {
    match std::env::var("JACKIN_PALETTE_KEY") {
        Err(_) => Some(0x1C),
        Ok(s) if s.eq_ignore_ascii_case("none") => None,
        Ok(s) => match parse_key_binding(&s) {
            Some(byte) => Some(byte),
            None => {
                crate::clog!("invalid JACKIN_PALETTE_KEY={s:?}; using default Ctrl+\\");
                Some(0x1C)
            }
        },
    }
}
