// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `tab_strip`.
use super::{TabStrip, tab_cell_style, tab_underline_line};
use crate::{components::HoverTracker, lay_out_tabs};
use ratatui::layout::Rect;

#[test]
fn underline_marks_only_active_tab_when_focused() {
    let cells = lay_out_tabs(&[("General", true), ("Mounts", false)], 0);

    let text: String = tab_underline_line(&cells, true)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert_eq!(text, "━━━━━━━━━          ");
}

#[test]
fn tab_strip_exposes_two_rows() {
    let labels = [("General", true), ("Mounts", false)];
    let backend = ratatui::backend::TestBackend::new(24, 2);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            frame.render_widget(TabStrip::new(&labels).focused(true), frame.area());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(0, 1)].symbol(), "━");
}

#[test]
fn tab_strip_exposes_rendered_cells_for_hit_testing() {
    let labels = [("General", true), ("Mounts", false)];
    let cells = TabStrip::new(&labels).cells(8);
    let expected = lay_out_tabs(&labels, 8);

    assert_eq!(cells.len(), expected.len());
    assert_eq!(cells[0].label, expected[0].label);
    assert_eq!(cells[0].active, expected[0].active);
    assert_eq!(cells[0].start_col, expected[0].start_col);
    assert_eq!(cells[0].cell_cols, expected[0].cell_cols);
    assert_eq!(cells[1].label, expected[1].label);
    assert_eq!(cells[1].active, expected[1].active);
    assert_eq!(cells[1].start_col, expected[1].start_col);
    assert_eq!(cells[1].cell_cols, expected[1].cell_cols);
}

#[test]
fn tab_strip_hit_index_uses_render_area() {
    let labels = [("General", true), ("Mounts", false)];
    let area = Rect {
        x: 8,
        y: 3,
        width: 40,
        height: 2,
    };

    assert_eq!(TabStrip::new(&labels).hit_index_at(area, 8, 3), Some(0));
    assert_eq!(TabStrip::new(&labels).hit_index_at(area, 18, 4), Some(1));
    assert_eq!(TabStrip::new(&labels).hit_index_at(area, 18, 5), None);
    assert_eq!(TabStrip::new(&labels).hit_index_at(area, 7, 3), None);
}

#[test]
fn tab_strip_registers_hover_targets_from_render_area() {
    let labels = [("General", true), ("Mounts", false)];
    let area = Rect {
        x: 8,
        y: 3,
        width: 40,
        height: 2,
    };
    let mut tracker = HoverTracker::new();

    TabStrip::new(&labels).register_hover_targets(&mut tracker, area, |idx| idx);

    assert_eq!(tracker.hovered(8, 3), Some(&0));
    assert_eq!(tracker.hovered(18, 4), Some(&1));
    assert_eq!(tracker.hovered(18, 5), None);
}

#[test]
fn tab_cell_style_centralizes_hover_colours() {
    assert_eq!(
        tab_cell_style(false, true).bg,
        Some(crate::theme::TAB_BG_INACTIVE_HOVER)
    );
    assert_eq!(
        tab_cell_style(true, true).bg,
        Some(crate::theme::TAB_BG_ACTIVE_HOVER)
    );
    assert_eq!(tab_cell_style(true, false).fg, Some(crate::theme::WHITE));
}
