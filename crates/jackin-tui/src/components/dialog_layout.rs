//! Shared dialog inner layout helper.
//!
//! Every modal dialog in jackin' follows the canonical vertical layout:
//!
//! ```text
//! ┌ Title ──────────────────────────────────────┐
//! │                                              │  ← 1 leading spacer row
//! │              content (1+ rows)              │
//! │                                              │  ← 1 spacer row
//! │          action / button row                 │
//! │                                              │  ← 1 trailing spacer row
//! └──────────────────────────────────────────────┘
//! ```
//!
//! Use `dialog_inner_chunks` to split the dialog's inner area according to
//! this canonical shape. The returned array has five slots:
//!
//! | Index | Contents                |
//! |-------|-------------------------|
//! | 0     | Leading spacer (1 row)  |
//! | 1     | Content area            |
//! | 2     | Spacer (1 row)          |
//! | 3     | Action / button row     |
//! | 4     | Trailing spacer (1 row) |

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Split `inner` into the canonical five-slot dialog layout.
///
/// `content_rows` is the number of content rows (slot 1). Pass `None` to use
/// `Min(1)` (the remaining space after the fixed rows are allocated), which is
/// correct for dialogs whose content height varies or is unknown.
#[must_use]
pub fn dialog_inner_chunks(inner: Rect, content_rows: Option<u16>) -> [Rect; 5] {
    let content = content_rows.map_or(Constraint::Min(1), Constraint::Length);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // leading spacer
            content,               // content
            Constraint::Length(1), // spacer
            Constraint::Length(1), // action row
            Constraint::Length(1), // trailing spacer
        ])
        .split(inner);
    [chunks[0], chunks[1], chunks[2], chunks[3], chunks[4]]
}

/// Minimum inner height needed for the canonical dialog layout with the given
/// content height. Add 2 for the dialog borders to get the total outer height.
#[must_use]
pub const fn dialog_inner_height(content_rows: u16) -> u16 {
    1u16.saturating_add(content_rows) // leading + content
        .saturating_add(1) // spacer
        .saturating_add(1) // action row
        .saturating_add(1) // trailing spacer
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn dialog_inner_height_accounts_for_all_five_slots() {
        // 1 leading + 1 content + 1 spacer + 1 action + 1 trailing = 5 inner rows
        assert_eq!(dialog_inner_height(1), 5);
        assert_eq!(dialog_inner_height(3), 7);
    }

    #[test]
    fn dialog_inner_chunks_returns_five_non_overlapping_rows() {
        let inner = Rect::new(0, 0, 60, 7);
        let chunks = dialog_inner_chunks(inner, Some(3));
        assert_eq!(chunks[0].height, 1, "leading spacer must be 1 row");
        assert_eq!(chunks[1].height, 3, "content must be 3 rows");
        assert_eq!(chunks[2].height, 1, "spacer must be 1 row");
        assert_eq!(chunks[3].height, 1, "action row must be 1 row");
        assert_eq!(chunks[4].height, 1, "trailing spacer must be 1 row");
        // Ensure all rows are vertically contiguous.
        assert_eq!(chunks[1].y, chunks[0].y + 1);
        assert_eq!(chunks[2].y, chunks[1].y + 3);
        assert_eq!(chunks[3].y, chunks[2].y + 1);
        assert_eq!(chunks[4].y, chunks[3].y + 1);
    }

    #[test]
    fn dialog_inner_chunks_leading_is_blank_trailing_is_blank() {
        // Slots 0 and 4 are spacers — they should be at the top and bottom of inner.
        let inner = Rect::new(2, 5, 50, 7);
        let chunks = dialog_inner_chunks(inner, Some(3));
        assert_eq!(
            chunks[0].y, inner.y,
            "leading spacer starts at top of inner"
        );
        assert_eq!(
            chunks[4].y + 1,
            inner.y + inner.height,
            "trailing spacer ends at bottom of inner"
        );
    }
}
