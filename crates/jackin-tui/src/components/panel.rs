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
            block = block.title(Span::styled(
                title,
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

/// A bordered `Block` for **modal overlays** — pickers, dialogs, and any
/// container that is the active interaction target when visible.
///
/// Always uses the focused (PHOSPHOR_GREEN) border style because modals are
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
/// Uses PHOSPHOR_DARK. For most cases, prefer `Panel::new().focus(PanelFocus::Unfocused).block()`
/// which also handles titles. This helper is for untitled containers only.
#[must_use]
pub fn unfocused_block<'a>() -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(PanelFocus::Unfocused.border_style())
}

#[cfg(test)]
mod tests {
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
        term.backend().buffer()[(0u16, 0u16)].style().fg.unwrap_or(Color::Reset)
    }

    #[test]
    fn modal_block_uses_phosphor_green() {
        assert_eq!(
            border_fg(modal_block()),
            Color::Rgb(0, 255, 65),
            "modal_block must use PHOSPHOR_GREEN so focused containers are visually distinct"
        );
    }

    #[test]
    fn unfocused_block_uses_phosphor_dark() {
        assert_eq!(
            border_fg(unfocused_block()),
            Color::Rgb(0, 80, 18),
            "unfocused_block must use PHOSPHOR_DARK"
        );
    }

    #[test]
    fn panel_focused_uses_phosphor_green() {
        assert_eq!(
            border_fg(Panel::new().focus(PanelFocus::Focused).block()),
            Color::Rgb(0, 255, 65),
            "PanelFocus::Focused must use PHOSPHOR_GREEN (WCAG focus-visible)"
        );
    }

    #[test]
    fn panel_unfocused_uses_phosphor_dark() {
        assert_eq!(
            border_fg(Panel::new().focus(PanelFocus::Unfocused).block()),
            Color::Rgb(0, 80, 18),
            "PanelFocus::Unfocused must use PHOSPHOR_DARK"
        );
    }
}
