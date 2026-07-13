// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared bordered panel primitive.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders},
};

use crate::theme::{PHOSPHOR_DARK, PHOSPHOR_GREEN, WHITE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelFocus {
    Unfocused,
    Focused,
    FocusedScrollable,
}

impl PanelFocus {
    const fn border_style(self) -> Style {
        match self {
            Self::Focused | Self::FocusedScrollable => Style::new().fg(PHOSPHOR_GREEN),
            Self::Unfocused => Style::new().fg(PHOSPHOR_DARK),
        }
    }
}

#[derive(Debug)]
pub struct Panel<'a> {
    title: Option<&'a str>,
    focus: PanelFocus,
}

impl<'a> Panel<'a> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            title: None,
            focus: PanelFocus::Unfocused,
        }
    }

    #[must_use]
    pub const fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    #[must_use]
    pub const fn focus(mut self, focus: PanelFocus) -> Self {
        self.focus = focus;
        self
    }

    #[must_use]
    pub fn block(self) -> Block<'a> {
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.focus.border_style());
        if let Some(title) = self.title {
            // Normalize to " Title " so callers never need to add padding manually.
            let padded = format!(" {} ", title.trim());
            block = block.title(Span::styled(
                padded,
                Style::new().fg(WHITE).add_modifier(Modifier::BOLD),
            ));
        }
        block
    }
}

impl Default for Panel<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the content area inside a panel border with a 1-cell horizontal inset so text
/// never touches the left or right border. Use in place of `block.inner(area)` when
/// rendering non-scrollable text content into a titled panel.
#[must_use]
pub fn panel_body_area(block: &Block<'_>, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Margin;
    block.inner(area).inner(Margin {
        horizontal: 1,
        vertical: 0,
    })
}

/// A bordered `Block` for **modal overlays** — pickers, dialogs, and any
/// container that is the active interaction target when visible.
///
/// Always uses the focused (`PHOSPHOR_GREEN`) border style because modals are
/// by definition the active container while they are open. Callers must not
/// construct `Block::default().borders(ALL).border_style(PHOSPHOR_DARK)` for
/// modals; use this helper instead so the correct color is the path of least
/// resistance and does not require per-call thinking.
///
/// For titled panels use `Panel::new().title("…").focus(PanelFocus::Focused).block()`.
/// For passive scroll blocks use `render_scrollable_block` which applies the
/// focus state automatically.
#[must_use]
pub fn modal_block<'a>() -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(PanelFocus::Focused.border_style())
}

/// A bordered `Block` for **unfocused** background containers.
///
/// Uses `PHOSPHOR_DARK`. For most cases, prefer `Panel::new().focus(PanelFocus::Unfocused).block()`
/// which also handles titles. This helper is for untitled containers only.
///
/// Also use this for background modals in a dialog stack. Only the topmost
/// dialog uses `modal_block()` (`PHOSPHOR_GREEN` border); every dialog beneath
/// uses this helper (`PHOSPHOR_DARK` border), so exactly one `PHOSPHOR_GREEN`
/// border is visible at a time.
#[must_use]
pub fn unfocused_block<'a>() -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(PanelFocus::Unfocused.border_style())
}

#[cfg(test)]
mod tests;
