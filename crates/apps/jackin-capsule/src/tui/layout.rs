// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(crate) const CAPSULE_HINT_TOP_SEPARATOR_ROWS: u16 = 1;

pub(crate) const CAPSULE_HINT_BAR_ROWS: u16 = 1;

pub(crate) const CAPSULE_HINT_SEPARATOR_ROWS: u16 = 1;

use crate::tui::components::branch_context_bar::BRANCH_CONTEXT_BAR_ROWS;
use crate::tui::components::status_bar::STATUS_BAR_ROWS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirectionGeometry {
    LeftRight,
    TopBottom,
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The rounded ratio is explicitly clamped to the representable non-negative u16 range."
)]
fn ratio_to_cells(size: u16, ratio: f32) -> u16 {
    (f32::from(size) * ratio)
        .round()
        .clamp(0.0, f32::from(u16::MAX)) as u16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
}

impl Rect {
    #[must_use]
    pub const fn new(row: u16, col: u16, rows: u16, cols: u16) -> Self {
        Self {
            row,
            col,
            rows,
            cols,
        }
    }

    #[must_use]
    pub const fn shrink(&self, n: u16) -> Self {
        let two_n = n.saturating_mul(2);
        let rows = self.rows.saturating_sub(two_n);
        let cols = self.cols.saturating_sub(two_n);
        let row = if self.rows >= two_n {
            self.row + n
        } else {
            self.row
        };
        let col = if self.cols >= two_n {
            self.col + n
        } else {
            self.col
        };
        Self {
            row,
            col,
            rows,
            cols,
        }
    }

    #[must_use]
    pub const fn contains(&self, inner: Self) -> bool {
        inner.row >= self.row
            && inner.col >= self.col
            && inner.row + inner.rows <= self.row + self.rows
            && inner.col + inner.cols <= self.col + self.cols
    }
}

#[must_use]
pub fn available_content_rows(term_rows: u16) -> u16 {
    term_rows
        .saturating_sub(STATUS_BAR_ROWS)
        .saturating_sub(BRANCH_CONTEXT_BAR_ROWS)
        .saturating_sub(CAPSULE_HINT_TOP_SEPARATOR_ROWS)
        .saturating_sub(CAPSULE_HINT_BAR_ROWS)
        .saturating_sub(CAPSULE_HINT_SEPARATOR_ROWS)
}

#[must_use]
pub fn content_rect(content_rows: u16, term_cols: u16) -> Rect {
    Rect::new(STATUS_BAR_ROWS, 0, content_rows, term_cols)
}

#[must_use]
pub fn split_spawn_inner_size(direction: SplitDirectionGeometry, from_rect: Rect) -> (u16, u16) {
    match direction {
        SplitDirectionGeometry::LeftRight => (
            from_rect.rows.saturating_sub(2),
            (from_rect.cols / 2).saturating_sub(2),
        ),
        SplitDirectionGeometry::TopBottom => (
            (from_rect.rows / 2).saturating_sub(2),
            from_rect.cols.saturating_sub(2),
        ),
    }
}

#[must_use]
pub fn local_mouse_position(inner: Rect, row: u16, col: u16) -> Option<(u16, u16)> {
    if row < inner.row || row >= inner.row + inner.rows {
        return None;
    }
    if col < inner.col || col >= inner.col + inner.cols {
        return None;
    }
    Some((row - inner.row, col - inner.col))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitPosition {
    Before,
    After,
}

mod resize;
mod tree;
pub use tree::PaneTree;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrient {
    Horizontal,
    Vertical,
}

pub const SPLIT_RATIO_MIN: f32 = 0.05;
pub const SPLIT_RATIO_MAX: f32 = 0.95;
pub const SPLIT_RATIO_DEFAULT: f32 = 0.5;

#[must_use]
pub fn clamp_split_ratio(r: f32) -> f32 {
    debug_assert!(
        r.is_finite(),
        "clamp_split_ratio called with non-finite {r}"
    );
    r.clamp(SPLIT_RATIO_MIN, SPLIT_RATIO_MAX)
}

#[derive(Debug, Clone)]
pub struct Tab {
    auto_label: String,
    custom_label: Option<String>,
    pub tree: PaneTree,
    pub focused_id: u64,
    pub zoomed: Option<u64>,
    pub codename: String,
    pub instance: Option<String>,
    pub account_id: Option<String>,
}

impl Tab {
    pub fn new_single(
        label: impl Into<String>,
        session_id: u64,
        codename: impl Into<String>,
    ) -> Self {
        Self {
            auto_label: label.into(),
            custom_label: None,
            tree: PaneTree::Leaf(session_id),
            focused_id: session_id,
            zoomed: None,
            codename: codename.into(),
            instance: None,
            account_id: None,
        }
    }

    #[must_use]
    pub fn label(&self) -> &str {
        self.custom_label.as_deref().unwrap_or(&self.auto_label)
    }

    #[must_use]
    pub fn label_owned(&self) -> String {
        self.label().to_owned()
    }

    #[must_use]
    pub fn custom_label(&self) -> Option<&str> {
        self.custom_label.as_deref()
    }

    pub fn set_custom_label(&mut self, label: String) {
        self.custom_label = if label.is_empty() { None } else { Some(label) };
    }

    pub fn reset_to_auto(&mut self) {
        self.custom_label = None;
    }

    pub(crate) fn set_auto_label(&mut self, label: String) {
        self.auto_label = label;
    }
}
