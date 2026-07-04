// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Shared Ratatui tab strip.

use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    TabCell,
    components::HoverTracker,
    lay_out_tabs, tab_at_column,
    theme::{TAB_BG_ACTIVE, TAB_BG_ACTIVE_HOVER, TAB_BG_INACTIVE, TAB_BG_INACTIVE_HOVER, WHITE},
};

#[derive(Debug, Clone, Copy)]
pub struct TabStrip<'a> {
    labels: &'a [(&'a str, bool)],
    focused: bool,
    hovered: Option<usize>,
}

impl<'a> TabStrip<'a> {
    #[must_use]
    pub const fn new(labels: &'a [(&'a str, bool)]) -> Self {
        Self {
            labels,
            focused: false,
            hovered: None,
        }
    }

    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    #[must_use]
    pub const fn hovered(mut self, hovered: Option<usize>) -> Self {
        self.hovered = hovered;
        self
    }

    pub fn render(self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(self.paragraph(), area);
    }

    /// Return the tab under a 0-based terminal coordinate.
    ///
    /// The same laid-out cells drive rendering, hover and click handling. The
    /// returned index is therefore the tab the operator sees under the pointer,
    /// not a locally re-derived approximation.
    #[must_use]
    pub fn hit_index_at(self, area: Rect, col: u16, row: u16) -> Option<usize> {
        if !area.contains(Position { x: col, y: row }) {
            return None;
        }
        let visible_height = area.height.min(2);
        if row >= area.y.saturating_add(visible_height) {
            return None;
        }
        tab_at_column(&self.cells(area.x), col)
    }

    /// Register each tab cell as a clickable hover region.
    ///
    /// Callers keep their own target enum, but the geometry comes from the tab
    /// strip itself. This keeps OSC-22 pointer cues, hover colour and clicks in
    /// the same coordinate system as the rendered strip.
    pub fn register_hover_targets<K, F>(
        self,
        tracker: &mut HoverTracker<K>,
        area: Rect,
        mut key_for: F,
    ) where
        K: Clone + PartialEq,
        F: FnMut(usize) -> K,
    {
        let height = area.height.min(2);
        if height == 0 {
            return;
        }
        for (idx, cell) in self.cells(area.x).iter().enumerate() {
            tracker.register(
                Rect {
                    x: cell.start_col,
                    y: area.y,
                    width: cell.cell_cols,
                    height,
                },
                key_for(idx),
            );
        }
    }

    #[must_use]
    pub fn paragraph(self) -> Paragraph<'static> {
        let cells = self.cells(0);
        Paragraph::new(vec![
            tab_label_line(&cells, self.hovered),
            tab_underline_line(&cells, self.focused),
        ])
    }

    #[must_use]
    pub fn cells(self, start_col: u16) -> Vec<TabCell<'a>> {
        lay_out_tabs(self.labels, start_col)
    }
}

#[must_use]
pub fn tab_label_line(cells: &[TabCell<'_>], hovered: Option<usize>) -> Line<'static> {
    let mut spans = Vec::with_capacity(cells.len().saturating_mul(2));
    for (idx, cell) in cells.iter().enumerate() {
        spans.push(Span::styled(
            format!(" {} ", cell.label),
            tab_cell_style(cell.active, hovered == Some(idx)),
        ));
        spans.push(Span::raw(" ".repeat(usize::from(crate::TAB_GAP))));
    }
    Line::from(spans)
}

#[must_use]
pub fn tab_cell_style(active: bool, hovered: bool) -> Style {
    let bg = match (active, hovered) {
        (true, true) => TAB_BG_ACTIVE_HOVER,
        (true, false) => TAB_BG_ACTIVE,
        (false, true) => TAB_BG_INACTIVE_HOVER,
        (false, false) => TAB_BG_INACTIVE,
    };
    if active {
        Style::default()
            .bg(bg)
            .fg(WHITE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(bg).fg(WHITE)
    }
}

#[must_use]
pub fn tab_underline_line(cells: &[TabCell<'_>], focused: bool) -> Line<'static> {
    let mut spans = Vec::with_capacity(cells.len().saturating_mul(2));
    for cell in cells {
        if focused {
            let bar_text = if cell.active {
                "━".repeat(usize::from(cell.cell_cols))
            } else {
                " ".repeat(usize::from(cell.cell_cols))
            };
            // Active tab underline uses PHOSPHOR_GREEN when tab bar is focused
            // — consistent with the "focused = bright green" rule across all
            // surfaces. WHITE was too subtle against the dark background.
            spans.push(Span::styled(
                bar_text,
                if cell.active {
                    crate::theme::GREEN
                } else {
                    Style::default()
                },
            ));
        } else {
            // Content is focused (tab bar is not). Show WHITE underline on the
            // active tab so the operator still sees which tab is selected, but
            // the dim color makes clear the tab bar itself is not the focus owner.
            // This gives a two-state visual: GREEN = tab bar active, WHITE = tab
            // bar inactive but showing context, blank = no tab shown at all.
            let bar_text = if cell.active {
                "━".repeat(usize::from(cell.cell_cols))
            } else {
                " ".repeat(usize::from(cell.cell_cols))
            };
            spans.push(Span::styled(
                bar_text,
                if cell.active {
                    Style::default().fg(WHITE)
                } else {
                    Style::default()
                },
            ));
        }
        spans.push(Span::raw(" ".repeat(usize::from(crate::TAB_GAP))));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests;
