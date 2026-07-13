// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `container_info`.
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend};

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
fn debug_info_puts_run_id_first_when_available() {
    let state = DebugInfo {
        jackin_version: Some("0.6.0-test".to_owned()),
        capsule_version: Some("0.6.0-capsule".to_owned()),
        container_id: Some("jk-test-container".to_owned()),
        role: Some("the-architect".to_owned()),
        agent: Some("Codex".to_owned()),
        target: Some("/workspace".to_owned()),
        run_id: Some("jk-run-top".to_owned()),
        diagnostics_log_path: Some("/tmp/jk-run-top.jsonl".to_owned()),
    }
    .into_state();

    let rows = state.rows();
    assert_eq!(rows.first().map(|row| row.label.as_str()), Some("Run ID"));
    assert_eq!(
        rows.first().map(ContainerInfoRow::value),
        Some("jk-run-top")
    );
    assert_eq!(
        state.keyboard_copy_payload(),
        Some((0, "jk-run-top".to_owned())),
        "keyboard copy defaults to the top Run ID row"
    );
}

#[test]
fn keyboard_copy_payload_uses_first_copyable_row() {
    let state = ContainerInfoState::new(
        "Debug info",
        vec![
            ContainerInfoRow::new("jackin version", "0.6.0-dev"),
            ContainerInfoRow::new("Run ID", "jk-run-123").copyable(),
            ContainerInfoRow::new("Diagnostics log", "/tmp/jk-run-123.jsonl").copyable(),
        ],
    );

    assert_eq!(
        state.keyboard_copy_payload(),
        Some((1, "jk-run-123".to_owned()))
    );
}

#[test]
fn enter_does_not_dismiss_container_info_state() {
    let mut state = ContainerInfoState::new(
        "Debug info",
        vec![ContainerInfoRow::new("Run ID", "jk-run-123").copyable()],
    );

    assert!(matches!(
        state.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        ModalOutcome::Continue
    ));
    assert!(matches!(
        state.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        ModalOutcome::Cancel
    ));
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
fn copy_payload_at_follows_horizontal_and_vertical_scroll() {
    let mut state = ContainerInfoState::new(
        "Debug info",
        vec![
            ContainerInfoRow::new("Container ID", "jk-hidden").copyable(),
            ContainerInfoRow::new("Run ID", "jk-run-hidden").copyable(),
            ContainerInfoRow::new("Role", "hidden-role"),
            ContainerInfoRow::new("Agent", "hidden-agent"),
            ContainerInfoRow::new("Target", "hidden-target"),
            ContainerInfoRow::new(
                "Diagnostics log",
                "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl",
            )
            .copyable(),
        ],
    );
    state.scroll.scroll_x = 16;
    state.scroll.scroll_y = 4;
    let area = Rect::new(0, 0, 50, 5);

    assert_eq!(
        copy_payload_at(area, &state, 5, 3),
        Some((
            5,
            "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl".to_owned()
        )),
        "copy hit-test must follow the value after both axes scroll"
    );
    assert_eq!(
        copy_payload_at(area, &state, 5, 1),
        None,
        "vertically scrolled-out rows must not remain clickable"
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
fn hyperlink_overlay_follows_horizontal_and_vertical_scroll() {
    let mut state = ContainerInfoState::new(
        "Debug info",
        vec![
            ContainerInfoRow::new("Container ID", "jk-hidden").copyable(),
            ContainerInfoRow::new("Run ID", "jk-run-hidden").copyable(),
            ContainerInfoRow::new("Role", "hidden-role"),
            ContainerInfoRow::new("Agent", "hidden-agent"),
            ContainerInfoRow::new("Target", "hidden-target"),
            ContainerInfoRow::new(
                "Diagnostics log",
                "/Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl",
            )
            .copyable()
            .hyperlink(
                "file:///Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl",
            ),
        ],
    );
    state.scroll.scroll_x = 58;
    state.scroll.scroll_y = 4;

    let overlay = String::from_utf8(hyperlink_overlay(Rect::new(0, 0, 50, 5), &state))
        .expect("overlay should be utf8");
    let visible = overlay
        .split("\x1b[1;4m")
        .nth(1)
        .and_then(|tail| tail.split("\x1b]8;;\x1b\\").next())
        .expect("overlay should include one visible linked text span");

    assert!(overlay.contains(
        "\u{1b}]8;;file:///Users/donbeave/.jackin-pr-495/data/diagnostics/runs/jk-run-b93735.jsonl\u{1b}\\"
    ));
    assert!(
        visible.contains("jk-run-b93735.jsonl"),
        "overlay must contain the horizontally visible diagnostics-log tail"
    );
    assert!(
        !visible.contains("/Users/donbeave"),
        "overlay must not link text that has scrolled off the left edge"
    );
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
