// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use ratatui::{Terminal, backend::TestBackend};
use termrock::widgets::{Panel, PanelChrome};

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
        .map(ratatui::buffer::Cell::symbol)
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
    let theme = termrock::style::DesignSystem::default();
    let panel = Panel::new(&theme)
        .title(state.title())
        .emphasis(PanelChrome::Focused);
    let table_area = detail_table_area(panel.inner(area));
    state.viewport = Some(area);
    let col = table_area.x.saturating_add(12);
    let row = table_area.y;
    let payload = copy_payload_at(area, &state, col, row);
    assert_eq!(payload, Some((0, "run-abc".to_owned())));
}

#[test]
fn required_height_scales_with_product_row_count() {
    let empty = ContainerInfoState::new("T", vec![]);
    assert_eq!(required_height(&empty), 7);
    let many: Vec<ContainerInfoRow> = (0..10)
        .map(|i| ContainerInfoRow::new(format!("R{i}"), format!("v{i}")))
        .collect();
    let tall = ContainerInfoState::new("T", many);
    assert_eq!(required_height(&tall), 14);
}

fn render_test_buffer(area: Rect, state: &ContainerInfoState) -> Buffer {
    let backend = TestBackend::new(48, 14);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_container_info(frame, area, state))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn find_text(buffer: &Buffer, area: Rect, expected: &str) -> Option<(u16, u16)> {
    for y in area.y..area.bottom() {
        for start_x in area.x..area.right() {
            let mut candidate = String::new();
            for x in start_x..area.right() {
                candidate.push_str(buffer[(x, y)].symbol());
                if candidate == expected {
                    return Some((start_x, y));
                }
                if !expected.starts_with(&candidate) {
                    break;
                }
            }
        }
    }
    None
}

#[test]
fn invisible_detail_surface_has_no_pointer_or_hyperlink_targets() {
    for area in [Rect::new(3, 2, 19, 10), Rect::new(3, 2, 40, 4)] {
        let mut state = ContainerInfoState::new(
            "Hidden details",
            vec![
                ContainerInfoRow::new("Key", "v42")
                    .copyable()
                    .hyperlink("https://x"),
            ],
        );
        state.mark_copied(0);
        state.set_hovered_row(Some(0));

        let buffer = render_test_buffer(area, &state);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                assert_eq!(buffer[(x, y)].symbol(), " ", "paint at ({x}, {y})");
                assert_eq!(copy_payload_at(area, &state, x, y), None);
                assert_eq!(hyperlink_payload_at(area, &state, x, y), None);
            }
        }
        assert!(hyperlink_regions(area, &state).is_empty());
        assert!(hyperlink_overlay(area, &state).is_empty());
    }
}

#[test]
fn minimum_visible_detail_surface_preserves_painted_copy_and_link_targets() {
    let area = Rect::new(4, 3, 20, 5);
    let state = ContainerInfoState::new(
        "Details",
        vec![
            ContainerInfoRow::new("KEY", "v42")
                .copyable()
                .hyperlink("https://x"),
        ],
    );
    let buffer = render_test_buffer(area, &state);
    let label = find_text(&buffer, area, "KEY").expect("painted label");
    let value = find_text(&buffer, area, "v42").expect("painted value");
    let copy_affordance = find_text(&buffer, area, "⧉").expect("painted copy affordance");
    assert_eq!(label.1, value.1);
    assert_eq!(copy_affordance.1, value.1);

    assert_eq!(
        copy_payload_at(area, &state, value.0, value.1),
        Some((0, "v42".to_owned()))
    );
    assert_eq!(
        copy_payload_at(area, &state, copy_affordance.0, copy_affordance.1),
        Some((0, "v42".to_owned()))
    );
    assert_eq!(
        hyperlink_payload_at(area, &state, value.0, value.1),
        Some((0, "https://x".to_owned()))
    );

    let regions = hyperlink_regions(area, &state);
    assert_eq!(regions.len(), 1);
    let (region, url) = &regions[0];
    assert_eq!(url, "https://x");
    let value_right = value.0 + 3;
    assert!(region.x <= value.0 && region.right() >= value_right);
    assert!(region.y <= value.1 && region.bottom() > value.1);
    assert!(region.x >= area.x && region.y >= area.y);
    assert!(region.right() <= area.right() && region.bottom() <= area.bottom());

    let overlay = hyperlink_overlay(area, &state);
    assert!(!overlay.is_empty());
    assert!(overlay.windows(4).any(|bytes| bytes == b"\x1b]8;"));
    assert!(
        overlay
            .windows(b"https://x".len())
            .any(|bytes| bytes == b"https://x")
    );
    assert!(overlay.windows(b"v42".len()).any(|bytes| bytes == b"v42"));
}
