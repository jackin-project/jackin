//! Shared bordered panel primitive.

use ratatui::{
    style::{Color, Modifier, Style},
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

    fn border_style_with_palette(self, palette: &FocusPalette) -> Style {
        match self {
            Self::Focused | Self::FocusedScrollable => Style::new().fg(palette.focused),
            Self::Unfocused => Style::new().fg(palette.unfocused),
        }
    }
}

/// Color palette for the two focus states of a `Panel` border.
///
/// The default palette uses the console's PHOSPHOR colors.
/// Surfaces with a different visual language (e.g. the in-container
/// capsule pane borders) can provide their own palette so the correct
/// colors are used without overriding the theme tokens globally.
#[derive(Debug, Clone, Copy)]
pub struct FocusPalette {
    /// Border color when the container is focused / active.
    pub focused: Color,
    /// Border color when the container is unfocused / background.
    pub unfocused: Color,
}

impl FocusPalette {
    /// Default console palette: PHOSPHOR_GREEN focused, PHOSPHOR_DARK unfocused.
    pub const CONSOLE: Self = Self {
        focused: PHOSPHOR_GREEN,
        unfocused: PHOSPHOR_DARK,
    };

    /// Capsule pane palette: near-white focused, medium-gray unfocused.
    /// Green focus rings inside the terminal are too distracting against
    /// agent output; a gray ramp is easier on the eye while still
    /// providing a clear focused/unfocused contrast.
    pub const CAPSULE_PANE: Self = Self {
        focused: Color::Rgb(180, 180, 180),
        unfocused: Color::Rgb(80, 80, 80),
    };
}

impl Default for FocusPalette {
    fn default() -> Self {
        Self::CONSOLE
    }
}

pub struct Panel<'a> {
    title: Option<&'a str>,
    focus: PanelFocus,
    palette: FocusPalette,
}

impl<'a> Panel<'a> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            title: None,
            focus: PanelFocus::Unfocused,
            palette: FocusPalette::CONSOLE,
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

    /// Override the default PHOSPHOR color palette with a custom one.
    /// Use `FocusPalette::CAPSULE_PANE` for surfaces where green focus
    /// rings would clash with the terminal content.
    #[must_use]
    pub const fn palette(mut self, palette: FocusPalette) -> Self {
        self.palette = palette;
        self
    }

    #[must_use]
    pub fn block(self) -> Block<'a> {
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(self.focus.border_style_with_palette(&self.palette));
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
mod tests;
