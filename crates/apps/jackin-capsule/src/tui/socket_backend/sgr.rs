// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! SGR escape encoding: color, underline metadata, and number writers.

use ratatui::style::Color;

use termpane::{Color as TermColor, UnderlineStyle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct SgrMetadata {
    pub(crate) underline_style: UnderlineStyle,
    pub(crate) underline_color: TermColor,
    pub(crate) overline: bool,
}

pub(crate) const fn term_color(color: TermColor) -> Color {
    match color {
        TermColor::Default => Color::Reset,
        TermColor::Idx(idx) => Color::Indexed(idx),
        TermColor::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

pub(crate) fn write_color_sgr(buf: &mut Vec<u8>, color: Color, is_bg: bool) {
    let base = if is_bg { 40u8 } else { 30u8 };
    match color {
        Color::Reset => {
            // Let the reset at the top of apply_style handle it.
        }
        Color::Black => push_sgr(buf, base),
        Color::Red => push_sgr(buf, base + 1),
        Color::Green => push_sgr(buf, base + 2),
        Color::Yellow => push_sgr(buf, base + 3),
        Color::Blue => push_sgr(buf, base + 4),
        Color::Magenta => push_sgr(buf, base + 5),
        Color::Cyan => push_sgr(buf, base + 6),
        Color::White => push_sgr(buf, base + 7),
        Color::DarkGray => push_sgr(buf, base + 60),
        Color::LightRed => push_sgr(buf, base + 61),
        Color::LightGreen => push_sgr(buf, base + 62),
        Color::LightYellow => push_sgr(buf, base + 63),
        Color::LightBlue => push_sgr(buf, base + 64),
        Color::LightMagenta => push_sgr(buf, base + 65),
        Color::LightCyan => push_sgr(buf, base + 66),
        Color::Gray => push_sgr(buf, base + 7),
        Color::Indexed(idx) => {
            buf.extend_from_slice(if is_bg { b"\x1b[48;" } else { b"\x1b[38;" });
            push_indexed_color_tail(buf, idx);
        }
        Color::Rgb(r, g, b) => {
            buf.extend_from_slice(if is_bg { b"\x1b[48;" } else { b"\x1b[38;" });
            push_rgb_color_tail(buf, r, g, b);
        }
    }
}

/// `5;<idx>m` tail of an indexed-color SGR, after the `38;`/`48;`/`58;` opener.
pub(crate) fn push_indexed_color_tail(buf: &mut Vec<u8>, idx: u8) {
    buf.extend_from_slice(b"5;");
    push_number(buf, u32::from(idx));
    buf.push(b'm');
}

/// `2;<r>;<g>;<b>m` tail of a truecolor SGR, after the `38;`/`48;`/`58;` opener.
pub(crate) fn push_rgb_color_tail(buf: &mut Vec<u8>, r: u8, g: u8, b: u8) {
    buf.extend_from_slice(b"2;");
    push_number(buf, u32::from(r));
    buf.push(b';');
    push_number(buf, u32::from(g));
    buf.push(b';');
    push_number(buf, u32::from(b));
    buf.push(b'm');
}

pub(crate) fn write_sgr_metadata(buf: &mut Vec<u8>, metadata: SgrMetadata) {
    match metadata.underline_style {
        UnderlineStyle::None => {}
        UnderlineStyle::Single => buf.extend_from_slice(b"\x1b[4m"),
        UnderlineStyle::Double => buf.extend_from_slice(b"\x1b[4:2m"),
        UnderlineStyle::Curly => buf.extend_from_slice(b"\x1b[4:3m"),
        UnderlineStyle::Dotted => buf.extend_from_slice(b"\x1b[4:4m"),
        UnderlineStyle::Dashed => buf.extend_from_slice(b"\x1b[4:5m"),
    }
    if metadata.underline_color != TermColor::Default {
        buf.extend_from_slice(b"\x1b[58;");
        match metadata.underline_color {
            TermColor::Default => {}
            TermColor::Idx(idx) => push_indexed_color_tail(buf, idx),
            TermColor::Rgb(r, g, b) => push_rgb_color_tail(buf, r, g, b),
        }
    }
    if metadata.overline {
        buf.extend_from_slice(b"\x1b[53m");
    }
}

pub(crate) fn push_sgr(buf: &mut Vec<u8>, code: u8) {
    buf.extend_from_slice(b"\x1b[");
    push_number(buf, u32::from(code));
    buf.push(b'm');
}

pub(crate) fn push_number(buf: &mut Vec<u8>, n: u32) {
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut remaining = n;
    loop {
        digits[len] = b'0' + (remaining % 10) as u8;
        len += 1;
        remaining /= 10;
        if remaining == 0 {
            break;
        }
    }
    for digit in digits[..len].iter().rev() {
        buf.push(*digit);
    }
}
