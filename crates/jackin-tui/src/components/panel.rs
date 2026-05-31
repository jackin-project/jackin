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
