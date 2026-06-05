//! Tests for `container_info`.
use ratatui::{Terminal, backend::TestBackend};

use super::*;

#[test]
fn renders_rows_with_title_and_link_style() {
    let state = ContainerInfoState::new(
        "Debug info",
        vec![
            ContainerInfoRow::new("Container ID", "jk-test")
                .copyable()
                .emphasised(),
            ContainerInfoRow::new("Run log", "~/.jackin/run.jsonl")
                .hyperlink("file:///tmp/run.jsonl"),
        ],
    );
    let backend = TestBackend::new(64, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_container_info(frame, frame.area(), &state))
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("Debug info"));
    assert!(rendered.contains("jk-test"));
    assert!(rendered.contains("~/.jackin/run.jsonl"));
}

#[test]
fn copy_payload_at_hits_copyable_value_column() {
    let state = ContainerInfoState::new(
        "Debug info",
        vec![
            ContainerInfoRow::new("Container ID", "jk-test")
                .copyable()
                .emphasised(),
            ContainerInfoRow::new("Run log", "~/.jackin/run.jsonl")
                .hyperlink("file:///tmp/run.jsonl"),
        ],
    );
    let area = Rect::new(0, 0, 64, 10);

    assert_eq!(
        copy_payload_at(area, &state, 18, 2),
        Some((0, "jk-test".to_string()))
    );
    assert_eq!(
        copy_payload_at(area, &state, 18, 3),
        None,
        "hyperlink-only rows are not copy targets"
    );
}

#[test]
fn hyperlink_overlay_emits_osc8_for_link_rows() {
    let state = ContainerInfoState::new(
        "Debug info",
        vec![
            ContainerInfoRow::new("Container ID", "jk-test").copyable(),
            ContainerInfoRow::new("Run log", "/tmp/run.jsonl").hyperlink("file:///tmp/run.jsonl"),
        ],
    );

    let overlay = String::from_utf8(hyperlink_overlay(Rect::new(0, 0, 64, 10), &state))
        .expect("overlay should be utf8");

    assert!(overlay.contains("\u{1b}]8;;file:///tmp/run.jsonl\u{1b}\\"));
    assert!(overlay.contains("/tmp/run.jsonl"));
    assert!(overlay.contains("\u{1b}]8;;\u{1b}\\"));
}

#[test]
fn long_value_shows_horizontal_scrollbar_and_scroll_reveals_tail() {
    // A value far wider than the dialog must not silently clip: a horizontal
    // scrollbar appears and scrolling right reveals the tail.
    let long = "/Users/donbeave/Projects/jackin-project/test/pr-495/.jackin/data/diagnostics/runs/jk-run-8e27f0.jsonl";
    let mut state = ContainerInfoState::new(
        "Debug info",
        vec![ContainerInfoRow::new("Diagnostics log", long).copyable()],
    );
    let backend = TestBackend::new(50, 8);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| render_container_info(frame, frame.area(), &state))
        .unwrap();
    let at_start = format!("{:?}", terminal.backend().buffer());
    assert!(
        at_start.contains('\u{2501}'),
        "horizontal scrollbar `━` must appear on overflow"
    );
    assert!(
        at_start.contains("/Users/donbeave"),
        "head visible at scroll 0"
    );
    assert!(
        !at_start.contains("jk-run-8e27f0"),
        "tail hidden at scroll 0"
    );

    // Scroll fully right; the tail becomes visible.
    state.scroll.scroll_x = u16::MAX; // clamped at render time
    terminal
        .draw(|frame| render_container_info(frame, frame.area(), &state))
        .unwrap();
    let scrolled = format!("{:?}", terminal.backend().buffer());
    assert!(
        scrolled.contains("jk-run-8e27f0"),
        "tail revealed after horizontal scroll"
    );
}

#[test]
fn short_content_shows_no_horizontal_scrollbar() {
    let state = ContainerInfoState::new(
        "Debug info",
        vec![ContainerInfoRow::new("jackin", "0.6.0-dev")],
    );
    let backend = TestBackend::new(60, 8);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render_container_info(frame, frame.area(), &state))
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(
        !rendered.contains('\u{2501}'),
        "no horizontal scrollbar when content fits"
    );
}
