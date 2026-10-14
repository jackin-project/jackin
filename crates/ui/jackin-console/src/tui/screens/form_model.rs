use ratatui::text::Line;

use crate::tui::components::editor_rows::{FieldEmphasis, labeled_field_line};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRow {
    pub label: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormSection {
    pub rows: Vec<FieldRow>,
    pub cursor: usize,
    pub show_cursor: bool,
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

    #[must_use]
    pub fn content_width(&self) -> usize {
        self.rows
            .iter()
            .map(|row| {
                let s = format!(
                    "  {:width$}{}",
                    row.label,
                    row.value,
                    width = self.label_width
                );
                termrock::text::display_cols(&s)
            })
            .max()
            .unwrap_or(0)
    }
}

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
