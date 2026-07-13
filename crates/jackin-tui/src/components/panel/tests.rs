// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `panel`.
use super::*;
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};

/// Render a block into a 3×3 terminal and return the fg color of the top-left
/// corner cell — that cell is always a border character, so its fg is the border color.
fn border_fg(block: Block<'_>) -> Color {
    let backend = TestBackend::new(3, 3);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        let area = Rect::new(0, 0, 3, 3);
        f.render_widget(block, area);
    })
    .unwrap();
    term.backend().buffer()[(0u16, 0u16)]
        .style()
        .fg
        .unwrap_or(Color::Reset)
}

#[test]
fn modal_block_uses_phosphor_green() {
    assert_eq!(
        border_fg(modal_block()),
        PHOSPHOR_GREEN,
        "modal_block must use PHOSPHOR_GREEN so focused containers are visually distinct"
    );
}

#[test]
fn unfocused_block_uses_phosphor_dark() {
    assert_eq!(
        border_fg(unfocused_block()),
        PHOSPHOR_DARK,
        "unfocused_block must use PHOSPHOR_DARK"
    );
}

#[test]
fn panel_focused_uses_phosphor_green() {
    assert_eq!(
        border_fg(Panel::new().focus(PanelFocus::Focused).block()),
        PHOSPHOR_GREEN,
        "PanelFocus::Focused must use PHOSPHOR_GREEN (WCAG focus-visible)"
    );
}

#[test]
fn panel_unfocused_uses_phosphor_dark() {
    assert_eq!(
        border_fg(Panel::new().focus(PanelFocus::Unfocused).block()),
        PHOSPHOR_DARK,
        "PanelFocus::Unfocused must use PHOSPHOR_DARK"
    );
}
