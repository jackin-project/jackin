// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

/// Renders one frame the way `inspect_surface_loop` does and returns the
/// visible diff rows. Single place naming the `TermRock` diff types.
fn draw(lines: &[String], scroll: &mut DiffScroll, width: u16, height: u16) -> Vec<String> {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use termrock::widgets::{DiffKind, DiffLine, DiffState, DiffView};

    // Mirrors run.rs: the render arm hands the widget `offset_for_render`
    // through the host scroll seam. A fresh state's scroll clamps against
    // zero metrics, so publish content/viewport before injecting. Head
    // `DiffView` always reserves a bottom chip row when the area is at least
    // three rows tall, so render one row taller and crop the chip row; the
    // pre-bump paint had no gutter/prefix chrome, so trim both ends.
    let mut state = DiffState::new();
    state.show_line_numbers = false;
    // No cursor on this surface — chrome suppressed as in run.rs.
    state.show_cursor = false;
    state
        .scroll_mut()
        .set_content_size(1, u16::try_from(lines.len()).unwrap_or(u16::MAX));
    state.scroll_mut().set_viewport(1, height);
    let offset = u16::try_from(scroll.offset_for_render(lines.len())).unwrap_or(u16::MAX);
    state.scroll_mut().set_offset_y(offset);
    let ids: Vec<String> = (0..lines.len())
        .map(|index| format!("diff-line-{index}"))
        .collect();
    let diff_lines: Vec<DiffLine<'_>> = lines
        .iter()
        .zip(&ids)
        .map(|(text, id)| DiffLine::new(id, DiffKind::Context, text))
        .collect();
    let theme = termrock::style::DesignSystem::default();
    let backend = TestBackend::new(width, height + 1);
    let mut term = Terminal::new(backend).expect("backend");
    term.draw(|f| {
        f.render_stateful_widget(&DiffView::new(&diff_lines, &theme), f.area(), &mut state);
    })
    .expect("draw");
    // ...and stores back the offset the widget kept after its viewport clamp.
    scroll.record_rendered(usize::from(state.offset()));
    let buf = term.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .collect()
}

/// 50 distinguishable diff lines, `L00` … `L49`.
fn fixture_lines() -> Vec<String> {
    (0..50).map(|i| format!("L{i:02}")).collect()
}

#[test]
fn trparity_diff_scroll_starts_at_top() {
    let lines = fixture_lines();
    let mut scroll = DiffScroll::new();
    let rows = draw(&lines, &mut scroll, 20, 10);
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0], "L00");
    assert_eq!(rows[9], "L09");
}

#[test]
fn trparity_diff_scroll_down_moves_window_one_line() {
    let lines = fixture_lines();
    let mut scroll = DiffScroll::new();
    let mut firsts = Vec::new();
    for _ in 0..3 {
        scroll.line_down(lines.len());
        firsts.push(draw(&lines, &mut scroll, 20, 10)[0].clone());
    }
    assert_eq!(firsts, ["L01", "L02", "L03"]);
}

#[test]
fn trparity_diff_scroll_up_clamps_at_top() {
    let lines = fixture_lines();
    let mut scroll = DiffScroll::new();
    scroll.line_down(lines.len());
    for _ in 0..3 {
        scroll.line_up();
    }
    let rows = draw(&lines, &mut scroll, 20, 10);
    assert_eq!(rows[0], "L00");
}

#[test]
fn trparity_diff_scroll_page_keys_move_ten_lines() {
    let lines = fixture_lines();
    let mut scroll = DiffScroll::new();
    scroll.page_down(lines.len());
    assert_eq!(draw(&lines, &mut scroll, 20, 10)[0], "L10");
    scroll.page_up();
    assert_eq!(draw(&lines, &mut scroll, 20, 10)[0], "L00");
}

#[test]
fn trparity_diff_scroll_bottom_window_shows_last_viewport_lines() {
    let lines = fixture_lines();
    let mut scroll = DiffScroll::new();
    for _ in 0..6 {
        scroll.page_down(lines.len());
    }
    let rows = draw(&lines, &mut scroll, 20, 10);
    // Key offset pinned at 49, but the widget clamps to max_offset(50, 10) = 40.
    assert_eq!(rows[0], "L40");
    assert_eq!(rows[9], "L49");
}

#[test]
fn trparity_diff_scroll_reset_returns_to_top() {
    let lines = fixture_lines();
    let mut scroll = DiffScroll::new();
    scroll.page_down(lines.len());
    scroll.page_down(lines.len());
    scroll.reset();
    let rows = draw(&lines, &mut scroll, 20, 10);
    assert_eq!(rows[0], "L00");
}

#[test]
fn trparity_diff_scroll_over_scroll_resumes_from_key_offset() {
    // Pre-bump two-offset behavior being characterized: the key offset (49
    // after six PageDowns) survives the widget's viewport clamp because only
    // the render offset is written back. One step up lands on key offset 48,
    // which the widget re-clamps to 40 — the window does not move.
    let lines = fixture_lines();
    let mut scroll = DiffScroll::new();
    for _ in 0..6 {
        scroll.page_down(lines.len());
    }
    assert_eq!(draw(&lines, &mut scroll, 20, 10)[0], "L40");
    scroll.line_up();
    assert_eq!(draw(&lines, &mut scroll, 20, 10)[0], "L40");
}
