// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Pane-tree structure, leaf queries, splits, removal, and focus moves.

use super::{Direction, Rect, SplitPosition, ratio_to_cells};

#[derive(Debug, Clone)]
pub enum PaneTree {
    Leaf(u64),
    HSplit {
        left: Box<PaneTree>,
        right: Box<PaneTree>,
        ratio: f32,
    },
    VSplit {
        top: Box<PaneTree>,
        bottom: Box<PaneTree>,
        ratio: f32,
    },
}

impl PaneTree {
    /// Walk the tree and return `(session_id, rect)` for every leaf.
    /// Each leaf's rect is the **outer** rectangle the pane occupies,
    /// including the cells the renderer paints its border on when
    /// the tab has more than one pane. The renderer subtracts a
    /// one-cell inset before laying out the agent's content.
    /// Adjacent panes share no gap — their borders sit immediately
    /// next to each other, matching zellij's `││` interior look.
    #[must_use]
    pub fn leaves(&self, rect: Rect) -> Vec<(u64, Rect)> {
        match self {
            Self::Leaf(id) => vec![(*id, rect)],
            Self::HSplit { left, right, ratio } => {
                let left_cols = ratio_to_cells(rect.cols, *ratio)
                    .max(1)
                    .min(rect.cols.saturating_sub(1));
                let right_cols = rect.cols - left_cols;
                let left_rect = Rect::new(rect.row, rect.col, rect.rows, left_cols);
                let right_rect = Rect::new(rect.row, rect.col + left_cols, rect.rows, right_cols);
                let mut v = left.leaves(left_rect);
                v.extend(right.leaves(right_rect));
                v
            }
            Self::VSplit { top, bottom, ratio } => {
                let top_rows = ratio_to_cells(rect.rows, *ratio)
                    .max(1)
                    .min(rect.rows.saturating_sub(1));
                let bot_rows = rect.rows - top_rows;
                let top_rect = Rect::new(rect.row, rect.col, top_rows, rect.cols);
                let bot_rect = Rect::new(rect.row + top_rows, rect.col, bot_rows, rect.cols);
                let mut v = top.leaves(top_rect);
                v.extend(bottom.leaves(bot_rect));
                v
            }
        }
    }

    /// Replace the leaf with `old_id` with an `HSplit`. `position`
    /// controls whether `new_id` lands on the left or right of
    /// `old_id`. Recurses into existing splits so nested layouts
    /// still find the target leaf.
    pub fn split_h(&mut self, old_id: u64, new_id: u64, position: SplitPosition) -> bool {
        match self {
            Self::Leaf(id) if *id == old_id => {
                let (left, right) = match position {
                    SplitPosition::Before => (new_id, old_id),
                    SplitPosition::After => (old_id, new_id),
                };
                *self = Self::HSplit {
                    left: Box::new(Self::Leaf(left)),
                    right: Box::new(Self::Leaf(right)),
                    ratio: 0.5,
                };
                true
            }
            Self::HSplit { left, right, .. } => {
                left.split_h(old_id, new_id, position) || right.split_h(old_id, new_id, position)
            }
            Self::VSplit { top, bottom, .. } => {
                top.split_h(old_id, new_id, position) || bottom.split_h(old_id, new_id, position)
            }
            Self::Leaf(_) => false,
        }
    }

    /// Replace the leaf with `old_id` with a `VSplit`. `position`
    /// controls whether `new_id` lands above or below `old_id`.
    pub fn split_v(&mut self, old_id: u64, new_id: u64, position: SplitPosition) -> bool {
        match self {
            Self::Leaf(id) if *id == old_id => {
                let (top, bottom) = match position {
                    SplitPosition::Before => (new_id, old_id),
                    SplitPosition::After => (old_id, new_id),
                };
                *self = Self::VSplit {
                    top: Box::new(Self::Leaf(top)),
                    bottom: Box::new(Self::Leaf(bottom)),
                    ratio: 0.5,
                };
                true
            }
            Self::HSplit { left, right, .. } => {
                left.split_v(old_id, new_id, position) || right.split_v(old_id, new_id, position)
            }
            Self::VSplit { top, bottom, .. } => {
                top.split_v(old_id, new_id, position) || bottom.split_v(old_id, new_id, position)
            }
            Self::Leaf(_) => false,
        }
    }

    /// When the removed leaf is a direct child of the **root** split,
    /// `remove_inner` returns `Some(sibling)` because there is no
    /// parent to splice the surviving subtree into. Apply that
    /// replacement here so the root is replaced with the sibling
    /// instead of remaining as `Self::Leaf(0)` (the sentinel value
    /// the inner removal uses for the swapped-out child).
    pub fn remove(&mut self, id: u64) -> bool {
        let (found, replacement) = self.remove_inner(id);
        if let Some(sibling) = replacement {
            *self = sibling;
        }
        found
    }

    pub(crate) fn remove_inner(&mut self, id: u64) -> (bool, Option<PaneTree>) {
        match self {
            Self::Leaf(lid) => {
                if *lid == id {
                    (true, None)
                } else {
                    (false, None)
                }
            }
            Self::HSplit { left, right, .. } => {
                if let Self::Leaf(lid) = left.as_ref()
                    && *lid == id
                {
                    let sibling = std::mem::replace(right.as_mut(), Self::Leaf(0));
                    return (true, Some(sibling));
                }
                if let Self::Leaf(rid) = right.as_ref()
                    && *rid == id
                {
                    let sibling = std::mem::replace(left.as_mut(), Self::Leaf(0));
                    return (true, Some(sibling));
                }
                let (found, replacement) = left.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        **left = r;
                    }
                    return (true, None);
                }
                let (found, replacement) = right.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        **right = r;
                    }
                    return (true, None);
                }
                (false, None)
            }
            Self::VSplit { top, bottom, .. } => {
                if let Self::Leaf(tid) = top.as_ref()
                    && *tid == id
                {
                    let sibling = std::mem::replace(bottom.as_mut(), Self::Leaf(0));
                    return (true, Some(sibling));
                }
                if let Self::Leaf(bid) = bottom.as_ref()
                    && *bid == id
                {
                    let sibling = std::mem::replace(top.as_mut(), Self::Leaf(0));
                    return (true, Some(sibling));
                }
                let (found, replacement) = top.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        **top = r;
                    }
                    return (true, None);
                }
                let (found, replacement) = bottom.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        **bottom = r;
                    }
                    return (true, None);
                }
                (false, None)
            }
        }
    }

    /// Find the leaf ID adjacent in direction from `from_id`, or None.
    #[must_use]
    pub fn adjacent(&self, rect: Rect, from_id: u64, dir: Direction) -> Option<u64> {
        let leaves = self.leaves(rect);
        let from_rect = leaves.iter().find(|(id, _)| *id == from_id)?.1;
        let (fr, fc) = (
            from_rect.row + from_rect.rows / 2,
            from_rect.col + from_rect.cols / 2,
        );
        let candidates: Vec<_> = leaves
            .iter()
            .filter(|(id, _)| *id != from_id)
            .filter(|(_, r)| match dir {
                Direction::Left => r.col + r.cols < fc,
                Direction::Right => r.col > fc,
                Direction::Up => r.row + r.rows < fr,
                Direction::Down => r.row > fr,
            })
            .collect();
        candidates
            .into_iter()
            .min_by_key(|(_, r)| {
                let cr = r.row + r.rows / 2;
                let cc = r.col + r.cols / 2;
                (i32::from(cr) - i32::from(fr)).unsigned_abs()
                    + (i32::from(cc) - i32::from(fc)).unsigned_abs()
            })
            .map(|(id, _)| *id)
    }

    #[must_use]
    pub fn all_ids(&self) -> Vec<u64> {
        match self {
            Self::Leaf(id) => vec![*id],
            Self::HSplit { left, right, .. } => {
                let mut v = left.all_ids();
                v.extend(right.all_ids());
                v
            }
            Self::VSplit { top, bottom, .. } => {
                let mut v = top.all_ids();
                v.extend(bottom.all_ids());
                v
            }
        }
    }

    /// Number of leaf panes, without allocating an id vector.
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf(_) => 1,
            Self::HSplit { left, right, .. } => left.leaf_count() + right.leaf_count(),
            Self::VSplit { top, bottom, .. } => top.leaf_count() + bottom.leaf_count(),
        }
    }
}
