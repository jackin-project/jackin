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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};

pub use crate::scroll::{ScrollAxes, ScrollAxis, mouse_scroll_delta};

/// Columns scrolled per horizontal wheel notch in a dialog body.
pub const DIALOG_HORIZONTAL_SCROLL_STEP: u16 = crate::scroll::DEFAULT_HORIZONTAL_SCROLL_STEP;

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
        self.handle_key_for_axes(
            key,
            content_height,
            viewport_height,
            content_width,
            viewport_width,
            ScrollAxes {
                vertical: crate::scroll::is_scrollable(content_height, viewport_height),
                horizontal: crate::scroll::is_scrollable(content_width, viewport_width),
            },
        )
    }

    pub fn handle_key_for_axes(
        &mut self,
        key: KeyEvent,
        content_height: usize,
        viewport_height: usize,
        content_width: usize,
        viewport_width: usize,
        axes: ScrollAxes,
    ) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k' | 'K') if axes.vertical => {
                self.scroll_y = self.scroll_y.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') if axes.vertical => {
                let max = content_height.saturating_sub(viewport_height) as u16;
                self.scroll_y = self.scroll_y.saturating_add(1).min(max);
                true
            }
            KeyCode::PageUp if axes.vertical => {
                self.scroll_y = self.scroll_y.saturating_sub(viewport_height as u16);
                true
            }
            KeyCode::PageDown if axes.vertical => {
                let max = content_height.saturating_sub(viewport_height) as u16;
                self.scroll_y = self
                    .scroll_y
                    .saturating_add(viewport_height as u16)
                    .min(max);
                true
            }
            KeyCode::Left | KeyCode::Char('h' | 'H') if axes.horizontal => {
                self.scroll_x = self.scroll_x.saturating_sub(1);
                true
            }
            KeyCode::Right | KeyCode::Char('l' | 'L') if axes.horizontal => {
                let max = content_width.saturating_sub(viewport_width) as u16;
                self.scroll_x = self.scroll_x.saturating_add(1).min(max);
                true
            }
            _ => false,
        }
    }

    /// Apply a crossterm mouse-scroll event to the offsets, returning `true` if
    /// it was a scroll the dialog consumed. Used by every surface's wheel
    /// handler so dialog wheel-scroll behaves identically everywhere.
    ///
    /// Horizontal scroll is `ScrollLeft` / `ScrollRight`, **or** `Shift` +
    /// `ScrollUp` / `ScrollDown` — some terminals map a horizontal trackpad
    /// swipe onto a shifted vertical wheel rather than emitting native
    /// horizontal-wheel events. Offsets are clamped at render time.
    pub fn on_mouse_scroll(&mut self, kind: MouseEventKind, modifiers: KeyModifiers) -> bool {
        self.on_mouse_scroll_for_axes(
            kind,
            modifiers,
            ScrollAxes {
                vertical: true,
                horizontal: true,
            },
        )
    }

    pub fn on_mouse_scroll_for_axes(
        &mut self,
        kind: MouseEventKind,
        modifiers: KeyModifiers,
        axes: ScrollAxes,
    ) -> bool {
        let Some(delta) = mouse_scroll_delta(kind, modifiers, axes) else {
            return false;
        };
        match delta.axis {
            ScrollAxis::Vertical => {
                crate::scroll::apply_delta_unclamped_u16(&mut self.scroll_y, delta.amount);
            }
            ScrollAxis::Horizontal => {
                crate::scroll::apply_delta_unclamped_u16(&mut self.scroll_x, delta.amount);
            }
        }
        true
    }

    /// Handle raw byte keys from surfaces that have already parsed keyboard
    /// input before crossing into shared TUI code. This mirrors
    /// [`Self::handle_key_for_axes`] for the capsule daemon's byte-oriented
    /// dialog loop.
    pub fn handle_raw_key_for_axes(&mut self, key: &[u8], axes: ScrollAxes) -> bool {
        match key {
            b"\x1b[A" | b"k" | b"K" if axes.vertical => {
                self.scroll_y = self.scroll_y.saturating_sub(1);
                true
            }
            b"\x1b[B" | b"j" | b"J" if axes.vertical => {
                self.scroll_y = self.scroll_y.saturating_add(1);
                true
            }
            b"\x1b[D" | b"h" | b"H" if axes.horizontal => {
                self.scroll_x = self.scroll_x.saturating_sub(1);
                true
            }
            b"\x1b[C" | b"l" | b"L" if axes.horizontal => {
                self.scroll_x = self.scroll_x.saturating_add(1);
                true
            }
            _ => false,
        }
    }

    /// Apply capsule/SGR wheel button bits to the shared dialog offsets. The
    /// button value uses bit0 for forward/down-or-right, bit1 for native
    /// horizontal, and bit2 for Shift's horizontal-wheel fallback.
    pub fn on_sgr_wheel_button_for_axes(&mut self, button: u8, axes: ScrollAxes) -> bool {
        let forward = (button & 1) != 0;
        let horizontal = (button & 2) != 0 || (button & 4) != 0;
        match (horizontal, forward) {
            (true, _) if !axes.horizontal => false,
            (false, _) if !axes.vertical => false,
            (true, true) => {
                self.scroll_x = self.scroll_x.saturating_add(DIALOG_HORIZONTAL_SCROLL_STEP);
                true
            }
            (true, false) => {
                self.scroll_x = self.scroll_x.saturating_sub(DIALOG_HORIZONTAL_SCROLL_STEP);
                true
            }
            (false, true) => {
                self.scroll_y = self.scroll_y.saturating_add(1);
                true
            }
            (false, false) => {
                self.scroll_y = self.scroll_y.saturating_sub(1);
                true
            }
        }
    }

    pub fn on_mouse_scroll_for_size(
        &mut self,
        kind: MouseEventKind,
        modifiers: KeyModifiers,
        content_height: usize,
        viewport_height: usize,
        content_width: usize,
        viewport_width: usize,
    ) -> bool {
        self.on_mouse_scroll_for_axes(
            kind,
            modifiers,
            ScrollAxes {
                vertical: crate::scroll::is_scrollable(content_height, viewport_height),
                horizontal: crate::scroll::is_scrollable(content_width, viewport_width),
            },
        )
    }

    /// Render vertical and/or horizontal scrollbars on the block border when needed.
    pub fn render_scrollbars(
        &self,
        frame: &mut Frame<'_>,
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
    frame: &mut Frame<'_>,
    block_area: Rect,
    content_area: Rect,
    lines: &[Line<'_>],
    scroll: &mut DialogBodyScroll,
) -> (usize, usize) {
    use crate::components::scrollable_panel::{effective_offset, line_width};

    // Real rendered width — NOT max_line_width, which mirrors a row's leading
    // indent as trailing scroll-pad (that is for the mounts *panel*, which
    // appends padding). A dialog body is scrolled by Paragraph::scroll with no
    // appended padding, so the padded width would let the body scroll past its
    // last column into blank and keep the thumb from sitting flush at the end.
    let content_width = lines.iter().map(line_width).max().unwrap_or(0);
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

/// Per-axis scroll availability for a dialog body whose scrollbars sit on
/// `block_area`'s border (the dialog's outer rect). Mirrors the scrollbar
/// `is_scrollable` gate exactly.
#[must_use]
pub fn dialog_scroll_axes(
    content_width: usize,
    content_height: usize,
    block_area: Rect,
) -> ScrollAxes {
    use crate::components::scrollable_panel::{is_scrollable, viewport_height, viewport_width};
    ScrollAxes {
        vertical: is_scrollable(content_height, viewport_height(block_area)),
        horizontal: is_scrollable(content_width, viewport_width(block_area)),
    }
}

/// Scroll-key hint spans reflecting *actual* per-axis availability: `↑↓←→` when
/// both axes overflow, `↑↓` for vertical-only, `←→` for horizontal-only, and an
/// empty vec when neither — so a surface never advertises an axis the operator
/// cannot move. The single source for the "scroll" hint vocabulary; every
/// scrollable dialog/overlay composes its full hint from this primitive plus
/// its own dismiss/copy keys.
#[must_use]
pub fn scroll_hint_spans(axes: ScrollAxes) -> Vec<crate::HintSpan<'static>> {
    let key = match (axes.vertical, axes.horizontal) {
        (true, true) => "↑↓←→",
        (true, false) => "↑↓",
        (false, true) => "←→",
        (false, false) => return Vec::new(),
    };
    vec![crate::HintSpan::Key(key), crate::HintSpan::Text("scroll")]
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
/// 2. Render the modal block (focused `PHOSPHOR_GREEN` border + title)
/// 3. Return the inner area for the caller to render content
///
/// Callers use `dialog_inner_chunks(inner, content_rows)` to lay out the
/// canonical five slots within the returned inner area.
#[must_use]
pub fn render_dialog_shell(frame: &mut Frame<'_>, area: Rect, title: Option<&str>) -> Rect {
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
    fn scroll_hint_spans_reflect_available_axes_only() {
        fn keys(axes: ScrollAxes) -> Vec<&'static str> {
            scroll_hint_spans(axes)
                .into_iter()
                .filter_map(|s| match s {
                    crate::HintSpan::Key(k) => Some(k),
                    _ => None,
                })
                .collect()
        }
        assert_eq!(
            keys(ScrollAxes {
                vertical: true,
                horizontal: true
            }),
            vec!["↑↓←→"]
        );
        assert_eq!(
            keys(ScrollAxes {
                vertical: true,
                horizontal: false
            }),
            vec!["↑↓"]
        );
        assert_eq!(
            keys(ScrollAxes {
                vertical: false,
                horizontal: true
            }),
            vec!["←→"]
        );
        assert!(
            scroll_hint_spans(ScrollAxes::none()).is_empty(),
            "no overflow → no scroll hint at all"
        );
    }

    #[test]
    fn dialog_scroll_axes_match_scrollbar_overflow_gate() {
        // 10-wide / 4-tall inner viewport (rect minus the 1-cell border each side).
        let rect = Rect::new(0, 0, 12, 6);
        // Fits both axes → no scroll advertised.
        assert_eq!(dialog_scroll_axes(10, 4, rect), ScrollAxes::none());
        // Wide content, short height → horizontal only.
        assert_eq!(
            dialog_scroll_axes(40, 4, rect),
            ScrollAxes {
                vertical: false,
                horizontal: true
            },
            "wide-but-short body must advertise ←→ only"
        );
        // Tall content, narrow → vertical only.
        assert_eq!(
            dialog_scroll_axes(10, 40, rect),
            ScrollAxes {
                vertical: true,
                horizontal: false
            }
        );
    }

    #[test]
    fn on_mouse_scroll_routes_axes_and_shift_fallback() {
        use crossterm::event::{KeyModifiers, MouseEventKind};
        let none = KeyModifiers::NONE;
        let shift = KeyModifiers::SHIFT;

        let mut s = DialogBodyScroll::new();
        assert!(s.on_mouse_scroll(MouseEventKind::ScrollDown, none));
        assert_eq!((s.scroll_x, s.scroll_y), (0, 1), "ScrollDown → vertical");
        assert!(s.on_mouse_scroll(MouseEventKind::ScrollRight, none));
        assert!(s.scroll_x > 0, "ScrollRight → horizontal");

        // Shift + vertical wheel is the horizontal fallback for terminals that
        // do not emit native horizontal-wheel events.
        let mut s2 = DialogBodyScroll::new();
        assert!(s2.on_mouse_scroll(MouseEventKind::ScrollDown, shift));
        assert_eq!(s2.scroll_y, 0, "Shift+ScrollDown must not move vertical");
        assert!(s2.scroll_x > 0, "Shift+ScrollDown → horizontal");

        // Non-scroll events are not consumed.
        let mut s3 = DialogBodyScroll::new();
        assert!(!s3.on_mouse_scroll(MouseEventKind::Moved, none));
    }

    #[test]
    fn key_scroll_ignores_axes_without_visible_scrollbars() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut scroll = DialogBodyScroll::new();

        assert!(
            !scroll.handle_key_for_axes(
                KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
                20,
                5,
                20,
                5,
                ScrollAxes {
                    vertical: false,
                    horizontal: true,
                },
            ),
            "Down must not be consumed when no vertical scrollbar is visible"
        );
        assert_eq!(scroll.scroll_y, 0);

        assert!(scroll.handle_key_for_axes(
            KeyEvent::new(KeyCode::Right, KeyModifiers::NONE),
            20,
            5,
            20,
            5,
            ScrollAxes {
                vertical: false,
                horizontal: true,
            },
        ));
        assert_eq!(scroll.scroll_x, 1);
    }

    #[test]
    fn mouse_scroll_ignores_axes_without_visible_scrollbars() {
        use crossterm::event::{KeyModifiers, MouseEventKind};
        let mut scroll = DialogBodyScroll::new();

        assert!(!scroll.on_mouse_scroll_for_axes(
            MouseEventKind::ScrollDown,
            KeyModifiers::NONE,
            ScrollAxes {
                vertical: false,
                horizontal: true,
            },
        ));
        assert_eq!((scroll.scroll_x, scroll.scroll_y), (0, 0));

        assert!(scroll.on_mouse_scroll_for_axes(
            MouseEventKind::ScrollRight,
            KeyModifiers::NONE,
            ScrollAxes {
                vertical: false,
                horizontal: true,
            },
        ));
        assert!(scroll.scroll_x > 0);
    }

    #[test]
    fn raw_key_scroll_uses_shared_axis_gates() {
        let mut scroll = DialogBodyScroll::new();
        let horizontal_only = ScrollAxes {
            vertical: false,
            horizontal: true,
        };

        assert!(!scroll.handle_raw_key_for_axes(b"\x1b[B", horizontal_only));
        assert_eq!(scroll.scroll_y, 0);
        assert!(scroll.handle_raw_key_for_axes(b"\x1b[C", horizontal_only));
        assert_eq!(scroll.scroll_x, 1);
        assert!(scroll.handle_raw_key_for_axes(b"\x1b[D", horizontal_only));
        assert_eq!(scroll.scroll_x, 0);
    }

    #[test]
    fn sgr_wheel_button_scroll_uses_shared_axis_gates() {
        let mut scroll = DialogBodyScroll::new();
        let horizontal_only = ScrollAxes {
            vertical: false,
            horizontal: true,
        };

        assert!(!scroll.on_sgr_wheel_button_for_axes(65, horizontal_only));
        assert_eq!((scroll.scroll_x, scroll.scroll_y), (0, 0));
        assert!(scroll.on_sgr_wheel_button_for_axes(67, horizontal_only));
        assert_eq!(scroll.scroll_y, 0);
        assert!(scroll.scroll_x > 0);
    }

    #[test]
    fn scrollable_body_shows_horizontal_bar_only_on_overflow_and_scroll_reveals_tail() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use ratatui::text::Line;

        fn render(lines: &[Line<'static>], scroll_x: u16) -> String {
            let mut terminal = Terminal::new(TestBackend::new(20, 6)).unwrap();
            terminal
                .draw(|frame| {
                    let area = Rect::new(0, 0, 20, 6);
                    let inner = Rect::new(1, 1, 18, 4);
                    let mut scroll = DialogBodyScroll {
                        scroll_x,
                        scroll_y: 0,
                    };
                    render_scrollable_dialog_body(frame, area, inner, lines, &mut scroll);
                })
                .unwrap();
            format!("{:?}", terminal.backend().buffer())
        }

        // Fits: no horizontal scrollbar.
        let short = [Line::from("abc")];
        assert!(!render(&short, 0).contains('\u{2501}'));

        // Overflows: bar appears, head visible, tail hidden until scrolled.
        let long = [Line::from("HEAD_0123456789_0123456789_0123456789_TAIL")];
        let at_start = render(&long, 0);
        assert!(at_start.contains('\u{2501}'), "overflow shows `━` bar");
        assert!(at_start.contains("HEAD"));
        assert!(!at_start.contains("TAIL"));
        assert!(
            render(&long, u16::MAX).contains("TAIL"),
            "scroll reveals tail"
        );
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
