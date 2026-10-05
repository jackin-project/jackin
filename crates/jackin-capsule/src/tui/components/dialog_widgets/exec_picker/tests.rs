// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
use super::*;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};
use std::sync::atomic::Ordering;

fn paint(state: &ExecPickerState, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| render(frame, Rect::new(0, 0, width, height), state))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

#[test]
fn exact_argv_render_preserves_controls_and_argument_boundaries() {
    let state = ExecPickerState::from_bindings(
        "gh".into(),
        vec![
            "repo".into(),
            "delete".into(),
            String::new(),
            "one two".into(),
            "\n\u{1b}[31m\u{009b}31m\u{202e}".into(),
        ],
        &[],
    );
    let expected = state.invocation.approval_argv();
    assert_eq!(argv_lines(&state, 7).concat(), expected);
    let buffer = paint(&state, 80, 20);
    let lines = argv_lines(&state, 78);
    for (index, line) in lines.iter().enumerate() {
        let rendered = row(&buffer, u16::try_from(index).unwrap() + 1);
        assert!(
            rendered
                .chars()
                .skip(1)
                .collect::<String>()
                .starts_with(line),
            "expected {line:?} in {rendered:?}"
        );
    }
    assert!(expected.contains("delete"));
    assert!(expected.contains("\\u009b"));
    assert!(expected.contains("\\u202e"));
    assert!(!expected.chars().any(char::is_control));
}

#[test]
fn long_argv_scroll_exposes_tail_and_returns_from_end() {
    let state = ExecPickerState::from_bindings(
        "tool".into(),
        vec!["x".repeat(500), "TRAILING-DESTRUCTIVE-ARG".into()],
        &[],
    );
    let initial = paint(&state, 48, 8);
    assert!((0..8).any(|y| row(&initial, y).contains("more argv")));
    state.argv_scroll.store(usize::MAX, Ordering::Relaxed);
    let end = paint(&state, 48, 8);
    let end_text = (0..8).map(|y| row(&end, y)).collect::<String>();
    assert!(end_text.contains("TRAILING-DESTRUCTIVE-ARG"));
    let max = state.argv_scroll.load(Ordering::Relaxed);
    assert!(max > 0 && max < usize::MAX);
    state.argv_scroll.fetch_sub(1, Ordering::Relaxed);
    let previous = paint(&state, 48, 8);
    assert_ne!(previous, end);
    assert!(row(&previous, 6).contains("more argv"));
}

#[test]
fn resize_clamps_scroll_using_current_exact_body_geometry() {
    let state = ExecPickerState::from_bindings("tool".into(), vec!["x".repeat(100)], &[]);
    state.argv_scroll.store(usize::MAX, Ordering::Relaxed);
    paint(&state, 20, 5);
    assert!(state.argv_scroll.load(Ordering::Relaxed) > 0);
    let resized = paint(&state, 120, 10);
    assert_eq!(state.argv_scroll.load(Ordering::Relaxed), 0);
    assert!(row(&resized, 1).contains(&state.invocation.approval_argv()));
}

#[test]
fn approval_distinguishes_spaced_argument_from_two_arguments() {
    let one = ExecPickerState::from_bindings("tool".into(), vec!["one two".into()], &[]);
    let two = ExecPickerState::from_bindings("tool".into(), vec!["one".into(), "two".into()], &[]);
    assert_ne!(paint(&one, 48, 8), paint(&two, 48, 8));
}

#[test]
fn one_column_viewport_exposes_every_unicode_argument_character() {
    let state =
        ExecPickerState::from_bindings("界".into(), vec!["e\u{0301}👩\u{200d}💻".into()], &[]);
    let expected = state.invocation.approval_argv();
    assert!(expected.is_ascii());
    assert_eq!(argv_lines(&state, 1).concat(), expected);
    let mut visible = String::new();
    for offset in 0..expected.len() {
        state.argv_scroll.store(offset, Ordering::Relaxed);
        let buffer = paint(&state, 3, 3);
        visible.push_str(buffer[(1, 1)].symbol());
    }
    assert_eq!(visible, expected);
    let decoded: Vec<String> = serde_json::from_str(&visible).unwrap();
    assert_eq!(decoded, vec!["界", "e\u{0301}👩\u{200d}💻"]);
}

#[test]
fn narrow_viewports_keep_overflow_direction_visible() {
    let state = ExecPickerState::from_bindings("tool".into(), vec!["x".repeat(100)], &[]);
    let two_body_rows = paint(&state, 3, 4);
    assert_eq!(two_body_rows[(1, 2)].symbol(), "↓");
    let one_body_row = paint(&state, 3, 3);
    assert_eq!(one_body_row[(1, 0)].symbol(), "↓");
    assert_eq!(one_body_row[(1, 1)].symbol(), "[");
    state.argv_scroll.store(usize::MAX, Ordering::Relaxed);
    let end = paint(&state, 3, 3);
    assert_eq!(end[(1, 0)].symbol(), "↑");
    assert_eq!(end[(1, 1)].symbol(), "]");
}
