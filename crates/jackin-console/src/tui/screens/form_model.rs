//! Shared form row / section view models for editor and settings screens.
//!
//! Names the data shape view builders pass around so both screens share
//! section rendering without multi-param state generics on every helper.

use ratatui::text::Line;

use crate::tui::components::editor_rows::{FieldEmphasis, labeled_field_line};

/// One labeled value row in a form section (editor general / settings general).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRow {
    /// Column label shown to the left of the value.
    pub label: String,
    /// Display string for the current value.
    pub value: String,
}

impl FieldRow {
    #[must_use]
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

/// Vertical list of [`FieldRow`]s with a single cursor for highlight.
///
/// Produced by both the workspace-editor general tab and the settings general
/// tab; pure line builders consume this so render paths stay free of the full
/// multi-param `EditorState` / `SettingsState` type trains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSection {
    pub rows: Vec<FieldRow>,
    pub cursor: usize,
    pub show_cursor: bool,
    /// Label column width for `labeled_field_line` rendering.
    pub label_width: usize,
}

impl FormSection {
    #[must_use]
    pub fn new(rows: Vec<FieldRow>, cursor: usize, show_cursor: bool, label_width: usize) -> Self {
        Self {
            rows,
            cursor,
            show_cursor,
            label_width,
        }
    }

    /// Render rows using the shared labeled-field helper.
    #[must_use]
    pub fn lines(&self) -> Vec<Line<'static>> {
        self.rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let selected = self.show_cursor && self.cursor == i;
                labeled_field_line(
                    selected,
                    "",
                    &row.label,
                    self.label_width,
                    row.value.clone(),
                    FieldEmphasis::SelectedValue,
                )
            })
            .collect()
    }

    /// Content width of the widest row (cursor gutter + padded label + value).
    #[must_use]
    pub fn content_width(&self) -> usize {
        self.rows
            .iter()
            .map(|row| {
                let s = format!("  {:width$}{}", row.label, row.value, width = self.label_width);
                jackin_tui::display_cols(&s)
            })
            .max()
            .unwrap_or(0)
    }
}

/// Scroll offsets + focus flag for a tab panel (shared by editor/settings tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabPanelView {
    pub scroll_x: u16,
    pub scroll_y: u16,
    pub focused: bool,
}

impl TabPanelView {
    #[must_use]
    pub const fn new(scroll_x: u16, scroll_y: u16, focused: bool) -> Self {
        Self {
            scroll_x,
            scroll_y,
            focused,
        }
    }
}
