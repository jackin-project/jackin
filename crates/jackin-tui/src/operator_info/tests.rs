// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use ratatui::{Terminal, backend::TestBackend};
use termrock::widgets::{Panel, PanelEmphasis};

#[test]
fn render_container_info_paints_product_title_and_row_labels() {
    let backend = TestBackend::new(48, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = ContainerInfoState::new(
        "Debug info",
        vec![
            ContainerInfoRow::new("Run ID", "run-abc").copyable(),
            ContainerInfoRow::new("jackin version", "0.6.0-dev"),
        ],
    );
    terminal
        .draw(|frame| {
            render_container_info(frame, frame.area(), &state);
        })
        .unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(
        text.contains("Debug info"),
        "product title missing from painted buffer: {text:?}"
    );
    assert!(
        text.contains("Run ID"),
        "product row label missing from painted buffer: {text:?}"
    );
    assert!(
        text.contains("run-abc"),
        "product row value missing from painted buffer: {text:?}"
    );
}

#[test]
fn copy_payload_at_returns_copyable_row_value() {
    let mut state = ContainerInfoState::new(
        "Debug info",
        vec![ContainerInfoRow::new("Run ID", "run-abc").copyable()],
    );
    let area = Rect::new(0, 0, 40, 10);
    // Prime viewport geometry the same way surfaces do after paint.
    let theme = termrock::Theme::default();
    let panel = Panel::new(&theme)
        .title(state.title())
        .emphasis(PanelEmphasis::Focused);
    let table_area = detail_table_area(panel.inner(area));
    state.viewport = Some(area);
    // Click roughly on first content row value cell (inside table body).
    let col = table_area.x.saturating_add(12);
    let row = table_area.y;
    let payload = copy_payload_at(area, &state, col, row);
    assert_eq!(payload, Some((0, "run-abc".to_owned())));
}

#[test]
fn required_height_scales_with_product_row_count() {
    let empty = ContainerInfoState::new("T", vec![]);
    // Floor is 7; only counts beyond that grow height (rows + 4).
    assert_eq!(required_height(&empty), 7);
    let many: Vec<ContainerInfoRow> = (0..10)
        .map(|i| ContainerInfoRow::new(format!("R{i}"), format!("v{i}")))
        .collect();
    let tall = ContainerInfoState::new("T", many);
    assert_eq!(required_height(&tall), 14);
}
