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

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

/// Shared dialog body scroll state.
///
/// Any dialog whose body may exceed its viewport uses this type to track
/// the current scroll offset. Attach it to the dialog's state struct, call
/// `handle_key` for keyboard scroll events, and `render_scrollbars` after
/// rendering the body content.
#[derive(Debug, Clone, Default)]
pub struct DialogBodyScroll {
    pub scroll_y: u16,
    pub scroll_x: u16,
}

impl DialogBodyScroll {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            scroll_y: 0,
            scroll_x: 0,
        }
    }

    /// Handle a key event for scrolling. Returns `true` if the key was consumed.
    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        content_height: usize,
        viewport_height: usize,
        content_width: usize,
        viewport_width: usize,
    ) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                self.scroll_y = self.scroll_y.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                let max = content_height.saturating_sub(viewport_height) as u16;
                self.scroll_y = self.scroll_y.saturating_add(1).min(max);
                true
            }
            KeyCode::PageUp => {
                self.scroll_y = self.scroll_y.saturating_sub(viewport_height as u16);
                true
            }
            KeyCode::PageDown => {
                let max = content_height.saturating_sub(viewport_height) as u16;
                self.scroll_y = self
                    .scroll_y
                    .saturating_add(viewport_height as u16)
                    .min(max);
                true
            }
            KeyCode::Left | KeyCode::Char('h' | 'H') => {
                self.scroll_x = self.scroll_x.saturating_sub(1);
                true
            }
            KeyCode::Right | KeyCode::Char('l' | 'L') => {
                let max = content_width.saturating_sub(viewport_width) as u16;
                self.scroll_x = self.scroll_x.saturating_add(1).min(max);
                true
            }
            _ => false,
        }
    }

    /// Render vertical and/or horizontal scrollbars on the block border when needed.
    pub fn render_scrollbars(
        &self,
        frame: &mut Frame,
        block_area: Rect,
        content_height: usize,
        content_width: usize,
    ) {
        use crate::components::scrollable_panel::{
            is_scrollable, render_horizontal_scrollbar, render_vertical_scrollbar,
        };
        if is_scrollable(
            content_height,
            crate::components::scrollable_panel::viewport_height(block_area),
        ) {
            render_vertical_scrollbar(frame, block_area, content_height, self.scroll_y);
        }
        if is_scrollable(
            content_width,
            crate::components::scrollable_panel::viewport_width(block_area),
        ) {
            render_horizontal_scrollbar(frame, block_area, content_width, self.scroll_x);
        }
    }
}

/// Render a dialog body (`lines`) into `content_area` with both-axis scroll,
/// and draw scrollbars on `block_area`'s border when the content overflows.
///
/// **This is THE shared mechanism for scrollable dialog bodies.** Every dialog
/// renders its line-based body through this helper so horizontal and vertical
/// scroll behave identically everywhere, and a scrollbar appears only when the
/// content exceeds the visible area. `content_area` is normally the dialog's
/// inner area (the full area inside the border); pass `block_area` as the outer
/// dialog rect so the scrollbars land on the dialog's own border and their
/// thumb extents match the content viewport.
///
/// The offsets in `scroll` are clamped to the content in place (so a shrunk
/// dialog never leaves the body scrolled past its end), and the clamped
/// `(content_width, content_height)` is returned so the caller can dispatch
/// scroll keys against the same extents the renderer measured.
pub fn render_scrollable_dialog_body(
    frame: &mut Frame,
    block_area: Rect,
    content_area: Rect,
    lines: &[Line<'_>],
    scroll: &mut DialogBodyScroll,
) -> (usize, usize) {
    use crate::components::scrollable_panel::{effective_offset, max_line_width};

    let content_width = max_line_width(lines);
    let content_height = lines.len();
    let vp_w = usize::from(content_area.width);
    let vp_h = usize::from(content_area.height);
    let eff_x = effective_offset(content_width, vp_w, scroll.scroll_x);
    let eff_y = effective_offset(content_height, vp_h, scroll.scroll_y);
    scroll.scroll_x = eff_x;
    scroll.scroll_y = eff_y;

    Paragraph::new(lines.to_vec())
        .scroll((eff_y, eff_x))
        .render(content_area, frame.buffer_mut());
    scroll.render_scrollbars(frame, block_area, content_height, content_width);
    (content_width, content_height)
}

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

/// Minimal dialog shell: renders backdrop + bordered block + returns the inner area.
///
/// This is the structural skeleton that all dialogs share:
/// 1. Clear the dialog area (hide the background content)  
/// 2. Render the modal block (focused PHOSPHOR_GREEN border + title)
/// 3. Return the inner area for the caller to render content
///
/// Callers use `dialog_inner_chunks(inner, content_rows)` to lay out the
/// canonical five slots within the returned inner area.
#[must_use]
pub fn render_dialog_shell(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    title: Option<&str>,
) -> ratatui::layout::Rect {
    use crate::components::panel::{Panel, PanelFocus, modal_block};
    use ratatui::widgets::Widget;

    ratatui::widgets::Clear.render(area, frame.buffer_mut());

    let block = if let Some(t) = title {
        Panel::new().title(t).focus(PanelFocus::Focused).block()
    } else {
        modal_block()
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
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
