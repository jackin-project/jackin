// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! CSI reconstruction + SGR color parsing helpers extracted from grid.rs.
#[allow(
    unused_imports,
    clippy::wildcard_imports,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
use super::*;

/// Rebuild a CSI sequence as bytes for passthrough or diagnostics.
pub fn reconstruct_csi(params: &vte::Params, intermediates: &[u8], final_byte: u8) -> Vec<u8> {
    use std::io::Write as _;
    let mut buf = b"\x1b[".to_vec();
    buf.extend_from_slice(intermediates);
    for (idx, sub) in params.iter().enumerate() {
        if idx > 0 {
            buf.push(b';');
        }
        for (jdx, n) in sub.iter().enumerate() {
            if jdx > 0 {
                buf.push(b':');
            }
            let _unused = write!(buf, "{n}");
        }
    }
    buf.push(final_byte);
    buf
}

/// Parse extended color from SGR params starting at `i`.
pub fn underline_style_from_sgr(style: u16) -> UnderlineStyle {
    match style {
        0 => UnderlineStyle::None,
        2 => UnderlineStyle::Double,
        3 => UnderlineStyle::Curly,
        4 => UnderlineStyle::Dotted,
        5 => UnderlineStyle::Dashed,
        // 1 (single) and any unknown SGR underline style
        _ => UnderlineStyle::Single,
    }
}

/// Parse extended color from either colon subparameters (`38:2:r:g:b`) or
/// semicolon parameters (`38;2;r;g;b`). Advances `i` for semicolon forms.
pub fn parse_sgr_color(current: &[u16], params: &[&[u16]], i: &mut usize) -> Option<Color> {
    if current.len() > 1 {
        return parse_sgr_color_values(&current[1..]);
    }
    if *i + 1 >= params.len() {
        return None;
    }
    let mode = params[*i + 1].first().copied().unwrap_or(0);
    match mode {
        5 => {
            if *i + 2 < params.len() {
                let idx = params[*i + 2].first().copied().unwrap_or(0).min(255) as u8;
                *i += 2;
                Some(Color::Idx(idx))
            } else {
                None
            }
        }
        2 => {
            if *i + 4 < params.len() {
                let r = params[*i + 2].first().copied().unwrap_or(0).min(255) as u8;
                let g = params[*i + 3].first().copied().unwrap_or(0).min(255) as u8;
                let b = params[*i + 4].first().copied().unwrap_or(0).min(255) as u8;
                *i += 4;
                Some(Color::Rgb(r, g, b))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse extended color from a flat SGR value list (`5;idx` or `2;r;g;b`).
pub fn parse_sgr_color_values(values: &[u16]) -> Option<Color> {
    match values.first().copied()? {
        5 => values.get(1).map(|idx| Color::Idx((*idx).min(255) as u8)),
        2 => {
            let start = if values.len() >= 5 && values[1] == 0 {
                2
            } else {
                1
            };
            let r = values.get(start).copied()?.min(255) as u8;
            let g = values.get(start + 1).copied()?.min(255) as u8;
            let b = values.get(start + 2).copied()?.min(255) as u8;
            Some(Color::Rgb(r, g, b))
        }
        _ => None,
    }
}
