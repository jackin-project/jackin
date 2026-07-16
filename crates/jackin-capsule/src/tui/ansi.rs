//! Capsule-local raw ANSI helpers.

use jackin_brand::Rgb;
use std::io::Write as _;
use termrock::{Theme, style::Role};

pub const RESET: &str = "\x1b[0m";
pub const BRAND_BANNER: &str = "\n  \x1b[1m\x1b[48;2;0;255;65m\x1b[38;2;0;0;0m jackin\x1b[38;2;255;255;255m❯\x1b[38;2;0;0;0m \x1b[0m\n";

pub fn rgb_fg(rgb: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b)
}

/// Resolve a semantic TermRock foreground for capsule-only raw ANSI output.
#[must_use]
pub fn role_rgb(role: Role) -> Rgb {
    match Theme::default().style(role).fg.unwrap_or_default() {
        ratatui::style::Color::Rgb(r, g, b) => Rgb::new(r, g, b),
        _ => Rgb::new(255, 255, 255),
    }
}

pub fn fg(buf: &mut Vec<u8>, rgb: Rgb) {
    let _unused = write!(buf, "\x1b[38;2;{};{};{}m", rgb.r, rgb.g, rgb.b);
}

pub fn emit_osc8_open(buf: &mut Vec<u8>, href: &str) {
    buf.extend_from_slice(b"\x1b]8;;");
    buf.extend_from_slice(href.as_bytes());
    buf.extend_from_slice(b"\x1b\\");
}

pub fn emit_osc8_close(buf: &mut Vec<u8>) {
    buf.extend_from_slice(b"\x1b]8;;\x1b\\");
}
