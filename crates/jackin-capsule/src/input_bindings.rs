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

/// Accept:
/// - `C-a` ... `C-z` (case-insensitive) - `Ctrl+letter`, maps to `0x01..=0x1A`
/// - `C-\` / `C-]` / `C-^` / `C-_` - `Ctrl+symbol`, maps to `0x1C..=0x1F`
/// - `C-Space` or `C-@` - `Ctrl+Space` / `Ctrl+@`, maps to `0x00`
/// - A single ASCII control byte in hex form `0xNN`
/// - A single literal byte
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

#[cfg(test)]
mod tests {
    use super::parse_key_binding;

    #[test]
    fn parse_key_binding_forms() {
        assert_eq!(parse_key_binding("C-a"), Some(0x01));
        assert_eq!(parse_key_binding("C-b"), Some(0x02));
        assert_eq!(parse_key_binding("c-z"), Some(0x1A));
        assert_eq!(parse_key_binding("0x02"), Some(0x02));
        assert_eq!(parse_key_binding("0X1B"), Some(0x1B));
        assert_eq!(parse_key_binding("Q"), Some(b'Q'));
        assert_eq!(parse_key_binding("nope"), None);
    }
}
