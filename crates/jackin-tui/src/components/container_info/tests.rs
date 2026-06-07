//! Tests for `container_info`.
use ratatui::{
    Terminal,
    backend::TestBackend,
    style::{Color, Style},
    widgets::{Paragraph, Widget},
};

use super::*;

#[test]
fn debug_info_keeps_run_id_bare_and_diagnostics_path_separate() {
    let state = DebugInfo {
        jackin_version: Some("0.6.0-test".to_owned()),
        run_id: Some("jk-run-b93735".to_owned()),
        diagnostics_log_path: Some(
            "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl".to_owned(),
        ),
        ..Default::default()
    }
    .into_state();
    let rows = state.rows();

    let run_row = rows
        .iter()
        .find(|row| row.label == "Run ID")
        .expect("Run ID row present");
    assert_eq!(run_row.value(), "jk-run-b93735");
    assert!(
        !run_row.value().contains(".jsonl"),
        "Run ID row must never contain the diagnostics JSONL path"
    );
    assert!(run_row.is_copyable());

    let log_row = rows
        .iter()
        .find(|row| row.label == "Diagnostics log")
        .expect("Diagnostics log row present");
    assert_eq!(
        log_row.value(),
        "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl"
    );
    assert_eq!(
        log_row.href(),
        Some("file:///Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl")
    );
    assert!(log_row.is_copyable());
}

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
        Some((0, "jk-test".to_owned()))
    );
    assert_eq!(
        copy_payload_at(area, &state, 18, 3),
        None,
        "hyperlink-only rows are not copy targets"
    );
    assert_eq!(
        copy_payload_at(area, &state, 27, 2),
        Some((0, "jk-test".to_owned())),
        "explicit copy affordance must hit the same copy payload as the value"
    );
}

#[test]
fn copyable_rows_render_explicit_copy_affordance() {
    let state = ContainerInfoState::new(
        "Debug info",
        vec![ContainerInfoRow::new("Run ID", "jk-run-b93735").copyable()],
    );
    let backend = TestBackend::new(64, 8);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| render_container_info(frame, frame.area(), &state))
        .unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(
        rendered.contains('⧉'),
        "copyable rows must render the shared copy affordance"
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
        vec![ContainerInfoRow::new("jackin version", "0.6.0-dev")],
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

#[test]
fn blank_render_clears_full_background_to_terminal_default() {
    let state = ContainerInfoState::new(
        "Debug info",
        vec![ContainerInfoRow::new("Container ID", "jk-test").copyable()],
    );
    let backend = TestBackend::new(64, 10);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|frame| {
            Paragraph::new("dirty background")
                .style(Style::default().fg(Color::Yellow).bg(Color::Red))
                .render(frame.area(), frame.buffer_mut());
            render_container_info_on_blank(frame, frame.area(), Rect::new(10, 2, 44, 6), &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].symbol(), " ");
    assert_eq!(buffer[(0, 0)].fg, Color::Reset);
    assert_eq!(buffer[(0, 0)].bg, Color::Reset);
    let rendered = format!("{buffer:?}");
    assert!(!rendered.contains("dirty background"));
    assert!(rendered.contains("Debug info"));
}
