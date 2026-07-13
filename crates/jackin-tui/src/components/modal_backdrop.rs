// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared modal backdrop.

use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
    widgets::Widget,
};

/// Fills the target area with the canonical dialog backdrop. The fill uses the
/// terminal's DEFAULT background (`theme::DIALOG_BACKDROP` = `Color::Reset`):
/// the cells still overwrite the chrome behind them (occlusion), but with the
/// operator's terminal background instead of forced pure black.
#[derive(Debug)]
pub struct ModalBackdrop;

impl Widget for ModalBackdrop {
    fn render(self, area: ratatui::layout::Rect, buf: &mut Buffer) {
        let bg = crate::theme::DIALOG_BACKDROP;
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                let cell = &mut buf[(x, y)];
                cell.set_char(' ');
                cell.set_bg(bg);
                cell.set_fg(Color::Reset);
                cell.modifier = Modifier::empty();
            }
        }
    }
}

#[cfg(test)]
mod tests;
