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

    /// Whether this rect contains the given (row, col) point (1-based).
    pub fn contains(self, r: u16, c: u16) -> bool {
        r >= self.row && r < self.row + self.rows && c >= self.col && c < self.col + self.cols
    }
}

impl PaneTree {
    /// Walk the tree and return `(session_id, rect)` for every leaf.
    pub fn leaves(&self, rect: Rect) -> Vec<(u64, Rect)> {
        match self {
            Self::Leaf(id) => vec![(*id, rect)],
            Self::HSplit { left, right, ratio } => {
                let left_cols = ((rect.cols as f32 * ratio).round() as u16).max(1);
                let right_cols = rect.cols.saturating_sub(left_cols + 1); // +1 for border
                let left_rect = Rect::new(rect.row, rect.col, rect.rows, left_cols);
                let right_rect =
                    Rect::new(rect.row, rect.col + left_cols + 1, rect.rows, right_cols);
                let mut v = left.leaves(left_rect);
                v.extend(right.leaves(right_rect));
                v
            }
            Self::VSplit { top, bottom, ratio } => {
                let top_rows = ((rect.rows as f32 * ratio).round() as u16).max(1);
                let bot_rows = rect.rows.saturating_sub(top_rows + 1);
                let top_rect = Rect::new(rect.row, rect.col, top_rows, rect.cols);
                let bot_rect = Rect::new(rect.row + top_rows + 1, rect.col, bot_rows, rect.cols);
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
    pub fn remove(&mut self, id: u64) -> bool {
        self.remove_inner(id).0
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
                if let Self::Leaf(lid) = left.as_ref() {
                    if *lid == id {
                        let sibling = std::mem::replace(right.as_mut(), Self::Leaf(0));
                        return (true, Some(sibling));
                    }
                }
                if let Self::Leaf(rid) = right.as_ref() {
                    if *rid == id {
                        let sibling = std::mem::replace(left.as_mut(), Self::Leaf(0));
                        return (true, Some(sibling));
                    }
                }
                let (found, replacement) = left.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        *left = Box::new(r);
                    }
                    return (true, None);
                }
                let (found, replacement) = right.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        *right = Box::new(r);
                    }
                    return (true, None);
                }
                (false, None)
            }
            Self::VSplit { top, bottom, .. } => {
                if let Self::Leaf(tid) = top.as_ref() {
                    if *tid == id {
                        let sibling = std::mem::replace(bottom.as_mut(), Self::Leaf(0));
                        return (true, Some(sibling));
                    }
                }
                if let Self::Leaf(bid) = bottom.as_ref() {
                    if *bid == id {
                        let sibling = std::mem::replace(top.as_mut(), Self::Leaf(0));
                        return (true, Some(sibling));
                    }
                }
                let (found, replacement) = top.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        *top = Box::new(r);
                    }
                    return (true, None);
                }
                let (found, replacement) = bottom.remove_inner(id);
                if found {
                    if let Some(r) = replacement {
                        *bottom = Box::new(r);
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

/// A named tab — each tab has a label and its own pane layout.
#[derive(Debug, Clone)]
pub struct Tab {
    pub label: String,
    pub tree: PaneTree,
    pub focused_id: u64,
}

impl Tab {
    pub fn new_single(label: impl Into<String>, session_id: u64) -> Self {
        Self {
            label: label.into(),
            tree: PaneTree::Leaf(session_id),
            focused_id: session_id,
        }
    }
}
