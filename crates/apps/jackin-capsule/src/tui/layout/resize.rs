// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Pane-resize paths: keyboard ratio nudge and drag-resize hit testing.

use super::{Direction, PaneTree, Rect, SplitOrient, clamp_split_ratio, ratio_to_cells};

impl PaneTree {
    /// Nudge the split ratio of the nearest split whose orientation
    /// matches `dir`. Walks the tree to find the deepest split that
    /// contains `leaf_id` on the side we want to grow / shrink, then
    /// adjusts its ratio by `delta` (positive = grow current pane,
    /// negative = shrink). Clamps to `[0.05, 0.95]` so neither child
    /// can collapse to zero cols / rows. Non-finite `delta` (NaN, ±∞)
    /// is rejected up front because `f32::clamp` on NaN returns NaN —
    /// a NaN ratio cast as `u16` collapses one child of the split.
    #[expect(
        clippy::excessive_nesting,
        reason = "Pane-tree resize walker: nested `if crosses_this` + signed- \
              delta computation + recursive parent + sibling + border-crossing \
              state-machine branches. The nesting is the per-pane resize \
              propagation protocol."
    )]
    pub fn resize(&mut self, leaf_id: u64, dir: Direction, delta: f32) -> bool {
        if !delta.is_finite() {
            return false;
        }
        match self {
            Self::Leaf(_) => false,
            Self::HSplit { left, right, ratio } => {
                let left_has = left.all_ids().contains(&leaf_id);
                if matches!(dir, Direction::Left | Direction::Right) {
                    // Only adjust this split's ratio when the
                    // requested direction crosses *this* split. If
                    // the leaf and the direction's target are both
                    // inside `left`, recurse — let the deeper split
                    // own the resize.
                    let crosses_this = if left_has {
                        matches!(dir, Direction::Right)
                    } else {
                        matches!(dir, Direction::Left)
                    };
                    if crosses_this {
                        let signed = if left_has { delta } else { -delta };
                        *ratio = clamp_split_ratio(*ratio + signed);
                        return true;
                    }
                }
                if left_has {
                    left.resize(leaf_id, dir, delta)
                } else {
                    right.resize(leaf_id, dir, delta)
                }
            }
            Self::VSplit { top, bottom, ratio } => {
                let top_has = top.all_ids().contains(&leaf_id);
                if matches!(dir, Direction::Up | Direction::Down) {
                    let crosses_this = if top_has {
                        matches!(dir, Direction::Down)
                    } else {
                        matches!(dir, Direction::Up)
                    };
                    if crosses_this {
                        let signed = if top_has { delta } else { -delta };
                        *ratio = clamp_split_ratio(*ratio + signed);
                        return true;
                    }
                }
                if top_has {
                    top.resize(leaf_id, dir, delta)
                } else {
                    bottom.resize(leaf_id, dir, delta)
                }
            }
        }
    }

    /// Walk the tree looking for a split whose interior boundary the
    /// operator clicked. With no inter-pane gap the boundary
    /// occupies two adjacent cells (the right border of the first
    /// child and the left border of the second); either is accepted.
    /// Returns `(path, orient, split_rect)` so the daemon can save
    /// enough state to re-apply the drag without re-walking on each
    /// motion event.
    #[must_use]
    pub fn border_at(
        &self,
        rect: Rect,
        row: u16,
        col: u16,
    ) -> Option<(Vec<u8>, SplitOrient, Rect)> {
        match self {
            Self::Leaf(_) => None,
            Self::HSplit { left, right, ratio } => {
                let left_cols = ratio_to_cells(rect.cols, *ratio)
                    .max(1)
                    .min(rect.cols.saturating_sub(1));
                let right_cols = rect.cols - left_cols;
                let left_rect = Rect::new(rect.row, rect.col, rect.rows, left_cols);
                let right_rect = Rect::new(rect.row, rect.col + left_cols, rect.rows, right_cols);
                let boundary_a = rect.col + left_cols - 1;
                let boundary_b = rect.col + left_cols;
                if row >= rect.row
                    && row < rect.row + rect.rows
                    && (col == boundary_a || col == boundary_b)
                {
                    return Some((Vec::new(), SplitOrient::Horizontal, rect));
                }
                if let Some((mut p, o, r)) = left.border_at(left_rect, row, col) {
                    p.insert(0, 0);
                    return Some((p, o, r));
                }
                if let Some((mut p, o, r)) = right.border_at(right_rect, row, col) {
                    p.insert(0, 1);
                    return Some((p, o, r));
                }
                None
            }
            Self::VSplit { top, bottom, ratio } => {
                let top_rows = ratio_to_cells(rect.rows, *ratio)
                    .max(1)
                    .min(rect.rows.saturating_sub(1));
                let bot_rows = rect.rows - top_rows;
                let top_rect = Rect::new(rect.row, rect.col, top_rows, rect.cols);
                let bot_rect = Rect::new(rect.row + top_rows, rect.col, bot_rows, rect.cols);
                let boundary_a = rect.row + top_rows - 1;
                let boundary_b = rect.row + top_rows;
                if col >= rect.col
                    && col < rect.col + rect.cols
                    && (row == boundary_a || row == boundary_b)
                {
                    return Some((Vec::new(), SplitOrient::Vertical, rect));
                }
                if let Some((mut p, o, r)) = top.border_at(top_rect, row, col) {
                    p.insert(0, 0);
                    return Some((p, o, r));
                }
                if let Some((mut p, o, r)) = bottom.border_at(bot_rect, row, col) {
                    p.insert(0, 1);
                    return Some((p, o, r));
                }
                None
            }
        }
    }

    /// Set the ratio of the split node at `path` (steps: `0` = left/top
    /// child, `1` = right/bottom). Returns `true` when the path
    /// resolved to a split. Used by the mouse-drag resize handler
    /// after `border_at` records the path.
    ///
    /// Non-finite values are rejected (NaN survives `f32::clamp`; a
    /// NaN ratio cast to `u16` collapses one child of the split).
    pub fn set_ratio_at(&mut self, path: &[u8], new_ratio: f32) -> bool {
        if !new_ratio.is_finite() {
            return false;
        }
        let clamped = clamp_split_ratio(new_ratio);
        if path.is_empty() {
            match self {
                Self::HSplit { ratio, .. } | Self::VSplit { ratio, .. } => {
                    *ratio = clamped;
                    return true;
                }
                Self::Leaf(_) => return false,
            }
        }
        let (step, rest) = (path[0], &path[1..]);
        match self {
            Self::HSplit { left, right, .. } => {
                if step == 0 {
                    left.set_ratio_at(rest, clamped)
                } else {
                    right.set_ratio_at(rest, clamped)
                }
            }
            Self::VSplit { top, bottom, .. } => {
                if step == 0 {
                    top.set_ratio_at(rest, clamped)
                } else {
                    bottom.set_ratio_at(rest, clamped)
                }
            }
            Self::Leaf(_) => false,
        }
    }
}
