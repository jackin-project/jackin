//! Capsule TUI layout helpers: compute panel rects from terminal dimensions
//! for the status bar, branch context bar, and session pane tree.
//!
//! Not responsible for: painting any widget (see `tui` render modules) or
//! tracking focus (see `daemon`).

/// Binary tree pane layout — same recursive split model as tmux.
///
/// Each node is either a Leaf (holds one session) or an HSplit/VSplit
/// that divides its rectangle between two child subtrees.
/// One row reserved for the persistent hint bar shown in the main pane view.
pub(crate) const CAPSULE_HINT_BAR_ROWS: u16 = 1;

/// One blank separator row between the hint bar and the branch context bar,
/// matching the console layout (hint → separator → chrome).
pub(crate) const CAPSULE_HINT_SEPARATOR_ROWS: u16 = 1;

use crate::tui::components::branch_context_bar::BRANCH_CONTEXT_BAR_ROWS;
use crate::tui::components::status_bar::STATUS_BAR_ROWS;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirectionGeometry {
    LeftRight,
    TopBottom,
}

/// A concrete rectangle in terminal coordinates (1-based row/col).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub row: u16,
    pub col: u16,
    pub rows: u16,
    pub cols: u16,
}

impl Rect {
    pub const fn new(row: u16, col: u16, rows: u16, cols: u16) -> Self {
        Self {
            row,
            col,
            rows,
            cols,
        }
    }

    /// Shrink the rectangle by `n` cells on every side. Clamps to a
    /// zero-area rect when the inset would invert the dimensions —
    /// callers downstream check `rows == 0 || cols == 0` and skip
    /// rendering in that case, so a zero rect is safer than a panic.
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

    /// True when `inner` lies within `self`, treating both as half-open
    /// `[row, row+rows)` × `[col, col+cols)` ranges — coincident far edges
    /// pass, so a rect contains itself. Sub-rectangle containment, not point
    /// membership. Used to assert that pane subdivision never escapes its
    /// content rect — e.g. a pane top can never rise above `content_rect.row`
    /// (`STATUS_BAR_ROWS`) into the status bar.
    #[must_use]
    pub const fn contains(&self, inner: Self) -> bool {
        inner.row >= self.row
            && inner.col >= self.col
            && inner.row + inner.rows <= self.row + self.rows
            && inner.col + inner.cols <= self.col + self.cols
    }
}

pub fn available_content_rows(term_rows: u16) -> u16 {
    term_rows
        .saturating_sub(STATUS_BAR_ROWS)
        .saturating_sub(BRANCH_CONTEXT_BAR_ROWS)
        .saturating_sub(CAPSULE_HINT_BAR_ROWS)
        .saturating_sub(CAPSULE_HINT_SEPARATOR_ROWS)
}

pub fn content_rect(content_rows: u16, term_cols: u16) -> Rect {
    Rect::new(STATUS_BAR_ROWS, 0, content_rows, term_cols)
}

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

pub fn local_mouse_position(inner: Rect, row: u16, col: u16) -> Option<(u16, u16)> {
    if row < inner.row || row >= inner.row + inner.rows {
        return None;
    }
    if col < inner.col || col >= inner.col + inner.cols {
        return None;
    }
    Some((row - inner.row, col - inner.col))
}

impl PaneTree {
    /// Walk the tree and return `(session_id, rect)` for every leaf.
    /// Each leaf's rect is the **outer** rectangle the pane occupies,
    /// including the cells the renderer paints its border on when
    /// the tab has more than one pane. The renderer subtracts a
    /// one-cell inset before laying out the agent's content.
    /// Adjacent panes share no gap — their borders sit immediately
    /// next to each other, matching zellij's `││` interior look.
    pub fn leaves(&self, rect: Rect) -> Vec<(u64, Rect)> {
        match self {
            Self::Leaf(id) => vec![(*id, rect)],
            Self::HSplit { left, right, ratio } => {
                let left_cols = ((f32::from(rect.cols) * ratio).round() as u16)
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
                let top_rows = ((f32::from(rect.rows) * ratio).round() as u16)
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

    fn remove_inner(&mut self, id: u64) -> (bool, Option<PaneTree>) {
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

    /// Nudge the split ratio of the nearest split whose orientation
    /// matches `dir`. Walks the tree to find the deepest split that
    /// contains `leaf_id` on the side we want to grow / shrink, then
    /// adjusts its ratio by `delta` (positive = grow current pane,
    /// negative = shrink). Clamps to `[0.05, 0.95]` so neither child
    /// can collapse to zero cols / rows. Non-finite `delta` (NaN, ±∞)
    /// is rejected up front because `f32::clamp` on NaN returns NaN —
    /// a NaN ratio cast as `u16` collapses one child of the split.
    #[allow(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Where the new pane lands relative to the existing pane when a
/// split fires. `Before` puts it left (for `split_h`) or above (for
/// `split_v`); `After` puts it right or below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitPosition {
    Before,
    After,
}

#[cfg(test)]
mod tests;

/// Orientation of a pane split. Used by the mouse-drag resize path
/// so the daemon knows whether the operator's drag delta should be
/// applied against `cols` (H-split) or `rows` (V-split).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitOrient {
    Horizontal,
    Vertical,
}

impl PaneTree {
    /// Walk the tree looking for a split whose interior boundary the
    /// operator clicked. With no inter-pane gap the boundary
    /// occupies two adjacent cells (the right border of the first
    /// child and the left border of the second); either is accepted.
    /// Returns `(path, orient, split_rect)` so the daemon can save
    /// enough state to re-apply the drag without re-walking on each
    /// motion event.
    pub fn border_at(
        &self,
        rect: Rect,
        row: u16,
        col: u16,
    ) -> Option<(Vec<u8>, SplitOrient, Rect)> {
        match self {
            Self::Leaf(_) => None,
            Self::HSplit { left, right, ratio } => {
                let left_cols = ((f32::from(rect.cols) * ratio).round() as u16)
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
                let top_rows = ((f32::from(rect.rows) * ratio).round() as u16)
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

/// Lower bound for a split ratio. 0.05 = 5% of the available cells,
/// the smallest size before the grid / agent UI starts mis-wrapping.
pub const SPLIT_RATIO_MIN: f32 = 0.05;
/// Upper bound — symmetric counterpart of `SPLIT_RATIO_MIN`.
pub const SPLIT_RATIO_MAX: f32 = 0.95;
/// Default ratio used by every `split_h` / `split_v` constructor.
pub const SPLIT_RATIO_DEFAULT: f32 = 0.5;

/// Clamp a ratio into `[SPLIT_RATIO_MIN, SPLIT_RATIO_MAX]`. NaN must be
/// rejected before this is called — `f32::clamp` propagates NaN, and
/// a NaN ratio cast to `u16` later collapses a pane.
pub fn clamp_split_ratio(r: f32) -> f32 {
    debug_assert!(
        r.is_finite(),
        "clamp_split_ratio called with non-finite {r}"
    );
    r.clamp(SPLIT_RATIO_MIN, SPLIT_RATIO_MAX)
}

/// `label()` returns `custom_label` when set, otherwise `auto_label`.
/// Mutators preserve that precedence; do not read fields directly.
#[derive(Debug, Clone)]
pub struct Tab {
    auto_label: String,
    custom_label: Option<String>,
    pub tree: PaneTree,
    pub focused_id: u64,
    /// Unique human-readable codename assigned at tab creation (e.g. `"badger"`).
    /// Never reassigned; persists across agent process restarts and context resets
    /// because it is a tab property, not a process property. Injected into every
    /// child process as `JACKIN_AGENT_CODENAME`.
    pub codename: String,
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
            codename: codename.into(),
        }
    }

    pub fn label(&self) -> &str {
        self.custom_label.as_deref().unwrap_or(&self.auto_label)
    }

    pub fn label_owned(&self) -> String {
        self.label().to_owned()
    }

    pub fn custom_label(&self) -> Option<&str> {
        self.custom_label.as_deref()
    }

    /// Set the operator's override. Empty input is treated as a
    /// request to revert to the auto-derived label; callers that want
    /// only the explicit-revert intent should use `reset_to_auto`.
    pub fn set_custom_label(&mut self, label: String) {
        self.custom_label = if label.is_empty() { None } else { Some(label) };
    }

    /// Clear the operator's override so the next `label()` read falls
    /// back to the auto-derived name.
    pub fn reset_to_auto(&mut self) {
        self.custom_label = None;
    }

    /// Daemon-internal: refresh the auto-derived label after a spawn /
    /// split / remove. `custom_label`, if set, still shadows this at
    /// display time.
    pub(crate) fn set_auto_label(&mut self, label: String) {
        self.auto_label = label;
    }
}
