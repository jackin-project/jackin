/// Binary tree pane layout — same recursive split model as tmux.
///
/// Each node is either a Leaf (holds one session) or an HSplit/VSplit
/// that divides its rectangle between two child subtrees.

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
                let left_cols = ((rect.cols as f32 * ratio).round() as u16)
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
                let top_rows = ((rect.rows as f32 * ratio).round() as u16)
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

    /// Replace the leaf with `old_id` with an HSplit of `old_id` and `new_id`.
    pub fn split_h(&mut self, old_id: u64, new_id: u64) -> bool {
        match self {
            Self::Leaf(id) if *id == old_id => {
                *self = Self::HSplit {
                    left: Box::new(Self::Leaf(old_id)),
                    right: Box::new(Self::Leaf(new_id)),
                    ratio: 0.5,
                };
                true
            }
            Self::HSplit { left, right, .. } => {
                left.split_h(old_id, new_id) || right.split_h(old_id, new_id)
            }
            Self::VSplit { top, bottom, .. } => {
                top.split_h(old_id, new_id) || bottom.split_h(old_id, new_id)
            }
            Self::Leaf(_) => false,
        }
    }

    /// Replace the leaf with `old_id` with a VSplit of `old_id` and `new_id`.
    pub fn split_v(&mut self, old_id: u64, new_id: u64) -> bool {
        match self {
            Self::Leaf(id) if *id == old_id => {
                *self = Self::VSplit {
                    top: Box::new(Self::Leaf(old_id)),
                    bottom: Box::new(Self::Leaf(new_id)),
                    ratio: 0.5,
                };
                true
            }
            Self::HSplit { left, right, .. } => {
                left.split_v(old_id, new_id) || right.split_v(old_id, new_id)
            }
            Self::VSplit { top, bottom, .. } => {
                top.split_v(old_id, new_id) || bottom.split_v(old_id, new_id)
            }
            Self::Leaf(_) => false,
        }
    }

    /// Remove a leaf and collapse the parent. Returns true if removed.
    ///
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
                (cr as i32 - fr as i32).unsigned_abs() + (cc as i32 - fc as i32).unsigned_abs()
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[cfg(test)]
mod rect_shrink_tests {
    use super::Rect;

    #[test]
    fn shrink_inside_normal_rect() {
        let r = Rect::new(5, 10, 20, 30);
        let s = r.shrink(1);
        assert_eq!((s.row, s.col, s.rows, s.cols), (6, 11, 18, 28));
    }

    #[test]
    fn shrink_clamps_to_zero_when_too_narrow() {
        let r = Rect::new(5, 10, 1, 1);
        let s = r.shrink(1);
        // Width and height drop to zero; row/col stay put so callers
        // get a valid (if empty) rectangle.
        assert_eq!((s.rows, s.cols), (0, 0));
        assert_eq!((s.row, s.col), (5, 10));
    }

    #[test]
    fn shrink_by_zero_is_noop() {
        let r = Rect::new(2, 3, 7, 11);
        let s = r.shrink(0);
        assert_eq!((s.row, s.col, s.rows, s.cols), (2, 3, 7, 11));
    }
}

#[cfg(test)]
mod border_at_tests {
    use super::{Direction, PaneTree, Rect, SplitOrient};

    #[test]
    fn border_at_horizontal_split_returns_path_and_orient() {
        let mut tree = PaneTree::Leaf(1);
        tree.split_h(1, 2);
        let rect = Rect::new(0, 0, 10, 20);
        // Boundary cols sit either side of col=10 (left=9, right=10).
        let hit = tree.border_at(rect, 5, 10).expect("boundary hit");
        let (path, orient, _) = hit;
        assert!(path.is_empty(), "boundary at the root split");
        assert_eq!(orient, SplitOrient::Horizontal);
    }

    #[test]
    fn border_at_vertical_split_returns_correct_orient() {
        let mut tree = PaneTree::Leaf(1);
        tree.split_v(1, 2);
        let rect = Rect::new(0, 0, 10, 20);
        // Boundary row at row=5.
        let hit = tree.border_at(rect, 5, 4).expect("boundary hit");
        assert_eq!(hit.1, SplitOrient::Vertical);
    }

    #[test]
    fn border_at_returns_none_for_pane_interior() {
        let mut tree = PaneTree::Leaf(1);
        tree.split_h(1, 2);
        let rect = Rect::new(0, 0, 10, 20);
        // Click at col 3 is inside the left pane, not on the
        // boundary.
        assert!(tree.border_at(rect, 5, 3).is_none());
    }

    #[test]
    fn set_ratio_at_clamps_to_safe_range() {
        let mut tree = PaneTree::Leaf(1);
        tree.split_h(1, 2);
        assert!(tree.set_ratio_at(&[], 0.001));
        if let PaneTree::HSplit { ratio, .. } = tree {
            assert!(ratio >= 0.05);
        } else {
            panic!("expected HSplit");
        }
    }

    #[test]
    fn set_ratio_at_rejects_nan_and_infinity() {
        let mut tree = PaneTree::Leaf(1);
        tree.split_h(1, 2);
        // `is_finite()` covers NaN AND ±∞ — both would survive
        // `f32::clamp` (NaN: stays NaN; ±∞: clamps to a bound but
        // would already have polluted intermediate arithmetic).
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!tree.set_ratio_at(&[], bad), "{bad} must be rejected");
            if let PaneTree::HSplit { ratio, .. } = tree {
                assert!(ratio.is_finite());
            } else {
                panic!("expected HSplit");
            }
        }
    }

    #[test]
    fn resize_rejects_non_finite_delta() {
        let mut tree = PaneTree::Leaf(1);
        tree.split_h(1, 2);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!tree.resize(1, Direction::Right, bad));
            if let PaneTree::HSplit { ratio, .. } = tree {
                assert!(ratio.is_finite());
            } else {
                panic!("expected HSplit");
            }
        }
    }

    #[test]
    fn remove_3_deep_collapses_correctly() {
        // Build: HSplit{ Leaf(1), VSplit{ HSplit{ Leaf(2), Leaf(3) }, Leaf(4) } }
        let mut tree = PaneTree::Leaf(1);
        assert!(tree.split_h(1, 2));
        assert!(tree.split_v(2, 4));
        assert!(tree.split_h(2, 3));
        // Removing leaf 3 should collapse its parent HSplit to Leaf(2).
        assert!(tree.remove(3));
        assert!(tree.all_ids().contains(&1));
        assert!(tree.all_ids().contains(&2));
        assert!(tree.all_ids().contains(&4));
        assert!(!tree.all_ids().contains(&3));
        // Removing leaf 4 collapses VSplit to its remaining child.
        assert!(tree.remove(4));
        assert!(tree.all_ids().contains(&1));
        assert!(tree.all_ids().contains(&2));
        assert!(!tree.all_ids().contains(&4));
        // Removing leaf 2 collapses root HSplit to Leaf(1).
        assert!(tree.remove(2));
        assert_eq!(tree.all_ids(), vec![1]);
    }

    // Direction is only referenced via the test alias to keep this
    // module's `use` block tidy; no runtime assertion needs it.
    #[allow(dead_code)]
    fn _direction_referenced(_: Direction) {}
}

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
                let left_cols = ((rect.cols as f32 * ratio).round() as u16)
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
                let top_rows = ((rect.rows as f32 * ratio).round() as u16)
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
/// the smallest size before vt100 / agent UI starts mis-wrapping.
pub const SPLIT_RATIO_MIN: f32 = 0.05;
/// Upper bound — symmetric counterpart of SPLIT_RATIO_MIN.
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

/// A named tab — each tab has a label and its own pane layout.
/// `custom_label` is set when the operator double-clicks a tab and
/// types a fixed name; while it is `Some`, `label` mirrors it and the
/// daemon's auto-deriver leaves the tab alone. Clearing `custom_label`
/// (rename to empty string) restores automatic naming.
#[derive(Debug, Clone)]
pub struct Tab {
    pub label: String,
    pub custom_label: Option<String>,
    pub tree: PaneTree,
    pub focused_id: u64,
}

impl Tab {
    pub fn new_single(label: impl Into<String>, session_id: u64) -> Self {
        Self {
            label: label.into(),
            custom_label: None,
            tree: PaneTree::Leaf(session_id),
            focused_id: session_id,
        }
    }
}
