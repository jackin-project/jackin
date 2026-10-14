// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::Session;

use crate::tui::pane_snapshot::RowSnapshot;

impl Session {
    pub fn scroll_by(&mut self, delta: i32) -> bool {
        let filled = self.scrollback_filled();
        let before = self.scrollback_offset();
        let mut tail = termrock::scroll::TailScroll::new(before);
        tail.scroll_by(filled, delta as isize);
        if tail.offset() == before {
            return false;
        }
        self.shadow_grid.set_scrollback(tail.offset());
        true
    }

    pub fn set_scrollback_offset(&mut self, offset: usize) -> bool {
        let before = self.scrollback_offset();
        self.shadow_grid.set_scrollback(offset);
        self.scrollback_offset() != before
    }

    #[must_use]
    pub fn scrollback_offset(&self) -> usize {
        self.shadow_grid.scrollback()
    }

    pub fn scroll_to_live(&mut self) {
        self.reset_scrollback_view();
    }

    pub fn clear_scrollback_and_request_screen_clear(&mut self) {
        self.scroll_to_live();
        self.shadow_grid.clear_scrollback();
        let _sent = self.send_input(b"\x0c");
    }

    #[must_use]
    pub fn scrollback_filled(&self) -> usize {
        self.shadow_grid.scrollback_len()
    }

    pub fn scrollback_counts(&mut self) -> (usize, usize) {
        (self.shadow_grid.scrollback_len(), 0)
    }

    pub(crate) fn reset_scrollback_view(&mut self) {
        self.shadow_grid.set_scrollback(0);
    }

    pub(crate) fn render_content_snapshot(&self, viewport_cols: u16) -> Vec<RowSnapshot> {
        crate::tui::pane_snapshot::pane_content_from_damagegrid(&self.shadow_grid, viewport_cols)
    }

    pub(crate) fn render_content_snapshot_range(
        &self,
        viewport_cols: u16,
        content_rows: std::ops::Range<usize>,
    ) -> Vec<RowSnapshot> {
        crate::tui::pane_snapshot::pane_content_range_from_damagegrid(
            &self.shadow_grid,
            viewport_cols,
            content_rows,
        )
    }

    pub(crate) fn diagnostic_tail(&self, max_rows: usize) -> Option<String> {
        if max_rows == 0 {
            return None;
        }
        let (_, cols) = self.shadow_grid.size();
        let mut lines: Vec<String> = self
            .render_content_snapshot(cols)
            .into_iter()
            .rev()
            .filter_map(|row| {
                let line = row.text_range(0, cols).trim_end().to_owned();
                (!line.trim().is_empty()).then_some(line)
            })
            .take(max_rows)
            .collect();
        lines.reverse();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    #[must_use]
    pub fn hyperlink_target_at_content_row(&self, row: usize, col: u16) -> Option<&str> {
        self.shadow_grid.hyperlink_target_at_content_row(row, col)
    }
}
