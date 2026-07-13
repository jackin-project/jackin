// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
        vec!["↑↓/j/k", "←→/h/l"]
    );
    assert_eq!(
        keys(ScrollAxes {
            vertical: true,
            horizontal: false
        }),
        vec!["↑↓/j/k"]
    );
    assert_eq!(
        keys(ScrollAxes {
            vertical: false,
            horizontal: true
        }),
        vec!["←→/h/l"]
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
