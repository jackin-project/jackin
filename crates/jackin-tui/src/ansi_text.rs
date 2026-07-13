// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! ANSI escape stripping and styled-span parsing for strings rendered in
//! terminal UI components.
//!
//! Not responsible for: layout geometry, widget rendering, or color palette
//! definitions.

use anstyle_parse::{DefaultCharAccumulator, Params, Parser, Perform};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

#[must_use]
pub fn strip_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut parser = Parser::<DefaultCharAccumulator>::default();
    let mut performer = PlainPerformer { output: Vec::new() };
    for &byte in bytes {
        parser.advance(&mut performer, byte);
    }
    performer.output
}

struct PlainPerformer {
    output: Vec<u8>,
}

impl Perform for PlainPerformer {
    fn print(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.output
            .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        if matches!(byte, b'\n' | b'\r' | b'\t') {
            self.output.push(byte);
        }
    }
}

#[allow(
    clippy::excessive_nesting,
    reason = "ANSI parser dispatch: per-event track (C0 / CSI / OSC / DCS / text / \
              ESC) with terminal-state branches (utf8 pending, esc buffered, dp \
              pending) nested through the perform-trait shape. Extracting per-state \
              sub-helpers would require re-passing the parser + performer mutable \
              borrows across fn boundaries."
)]
pub fn styled_spans(input: &str, default_style: Style) -> Vec<Span<'static>> {
    let mut parser = Parser::<DefaultCharAccumulator>::default();
    let mut performer = StyledPerformer {
        default_style,
        style: default_style,
        spans: Vec::new(),
        current: String::new(),
    };
    for &byte in input.as_bytes() {
        parser.advance(&mut performer, byte);
    }
    performer.flush();
    performer.spans
}

struct StyledPerformer {
    default_style: Style,
    style: Style,
    spans: Vec<Span<'static>>,
    current: String,
}

impl StyledPerformer {
    fn flush(&mut self) {
        if self.current.is_empty() {
            return;
        }
        self.spans
            .push(Span::styled(std::mem::take(&mut self.current), self.style));
    }

    fn set_style(&mut self, style: Style) {
        if self.style != style {
            self.flush();
            self.style = style;
        }
    }
}

impl Perform for StyledPerformer {
    fn print(&mut self, c: char) {
        self.current.push(c);
    }

    fn execute(&mut self, byte: u8) {
        if byte == b'\t' {
            self.current.push('\t');
        }
    }

    #[allow(
        clippy::excessive_nesting,
        reason = "vte::Perform trait dispatcher (`csi_dispatch`) for the styled- \
                  spans parser requires a single exhaustive match on the SGR \
                  parameter byte covering every CSI SGR sequence the styled- \
                  spans consumer supports. Extracting per-SGR-code sub-dispatchers \
                  would require re-borrowing the parser state across fn \
                  boundaries — same constraint as jackin-term's csi_dispatch."
    )]
    fn csi_dispatch(&mut self, params: &Params, _intermediates: &[u8], _ignore: bool, action: u8) {
        if action != b'm' {
            return;
        }
        let mut values: Vec<u16> = params.iter().flatten().copied().collect();
        if values.is_empty() {
            values.push(0);
        }
        let mut style = self.style;
        let mut i = 0;
        while i < values.len() {
            let value = values[i];
            match value {
                0 => style = self.default_style,
                1 => style = style.add_modifier(Modifier::BOLD),
                2 => style = style.add_modifier(Modifier::DIM),
                22 => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
                30..=37 => style = style.fg(ansi_color(value - 30, false)),
                39 => style = style.fg(self.default_style.fg.unwrap_or(Color::Reset)),
                40..=47 => style = style.bg(ansi_color(value - 40, false)),
                49 => style = style.bg(self.default_style.bg.unwrap_or(Color::Reset)),
                90..=97 => style = style.fg(ansi_color(value - 90, true)),
                100..=107 => style = style.bg(ansi_color(value - 100, true)),
                38 | 48 => {
                    if let Some((color, consumed)) = parse_extended_color(&values[i + 1..]) {
                        style = if value == 38 {
                            style.fg(color)
                        } else {
                            style.bg(color)
                        };
                        i += consumed;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        self.set_style(style);
    }
}

const fn ansi_color(index: u16, bright: bool) -> Color {
    match (index, bright) {
        (0, false) => Color::Black,
        (1, false) => Color::Red,
        (2, false) => Color::Green,
        (3, false) => Color::Yellow,
        (4, false) => Color::Blue,
        (5, false) => Color::Magenta,
        (6, false) => Color::Cyan,
        (7, false) => Color::Gray,
        (0, true) => Color::DarkGray,
        (1, true) => Color::LightRed,
        (2, true) => Color::LightGreen,
        (3, true) => Color::LightYellow,
        (4, true) => Color::LightBlue,
        (5, true) => Color::LightMagenta,
        (6, true) => Color::LightCyan,
        (7, true) => Color::White,
        _ => Color::Reset,
    }
}

fn parse_extended_color(values: &[u16]) -> Option<(Color, usize)> {
    match values {
        [5, idx, ..] => Some((Color::Indexed((*idx).min(255) as u8), 2)),
        [2, r, g, b, ..] => Some((
            Color::Rgb(
                (*r).min(255) as u8,
                (*g).min(255) as u8,
                (*b).min(255) as u8,
            ),
            4,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
