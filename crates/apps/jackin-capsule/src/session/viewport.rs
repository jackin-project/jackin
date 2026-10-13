// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Scrollback, viewport rendering, and content queries.

use super::Session;

use crate::tui::pane_snapshot::RowSnapshot;

impl Session {
    /// Scroll the view by `delta` lines. Positive = scroll up (into
    /// history); negative = scroll down (toward live tail).
    ///
    /// Up-scroll is clamped to the **actual filled scrollback** at
    /// call time so scrolling past the top does not inflate the
    /// offset past what the grid will render.
    pub fn scroll_by(&mut self, delta: i32) -> bool {
        // Capsule panes scroll a PTY/DamageGrid tail view, not a top-offset
        // ratatui panel. `TailScroll` is the shared adapter for this shape;
        // the `scrollable_panel` offset helpers remain for ordinary widgets.
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

    /// Jump the scrollback view to an absolute tail-relative offset
    /// (`0` = live). Used by scrollbar click-to-jump; wheel deltas go
    /// through `scroll_by`.
    pub fn set_scrollback_offset(&mut self, offset: usize) -> bool {
        let before = self.scrollback_offset();
        self.shadow_grid.set_scrollback(offset);
        self.scrollback_offset() != before
    }

    /// Tail-relative scrollback view offset. The grid is the single owner
    /// (D12); the session only delegates.
    #[must_use]
    pub fn scrollback_offset(&self) -> usize {
        self.shadow_grid.scrollback()
    }

    /// Drop scrollback view, return to the live tail.
    pub fn scroll_to_live(&mut self) {
        self.reset_scrollback_view();
    }

    /// Clear this pane's saved scrollback and ask the foreground
    /// program to redraw its visible screen via the standard form-feed
    /// key (`Ctrl+L`). The visible grid is left to the PTY program so
    /// readline/TUI cursor state does not desynchronise from jackin❯'s
    /// local grid mirror.
    pub fn clear_scrollback_and_request_screen_clear(&mut self) {
        self.scroll_to_live();
        self.shadow_grid.clear_scrollback();
        let _sent = self.send_input(b"\x0c");
    }

    /// Number of scrollback lines currently retained for this pane.
    #[must_use]
    pub fn scrollback_filled(&self) -> usize {
        self.shadow_grid.scrollback_len()
    }

    /// Scrollback counts as `(grid_filled, inline_filled)`. The grid is
    /// the only scrollback source now, so the second element is always
    /// `0`; the tuple shape is kept for the debug-log call sites that
    /// still split the two for the `--debug` scrollbar trace.
    pub fn scrollback_counts(&mut self) -> (usize, usize) {
        (self.shadow_grid.scrollback_len(), 0)
    }

    pub(crate) fn reset_scrollback_view(&mut self) {
        self.shadow_grid.set_scrollback(0);
    }

    pub(crate) fn render_content_snapshot(&self, viewport_cols: u16) -> Vec<RowSnapshot> {
        crate::tui::pane_snapshot::pane_content_from_damagegrid(&self.shadow_grid, viewport_cols)
    }

    /// Content-coordinate snapshot of `content_rows` only (half-open).
    /// Element `i` is absolute content row `content_rows.start + i` (after clamp).
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
