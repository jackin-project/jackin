// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `github_picker`.
use super::*;
use crate::github_mounts::GithubChoice;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn choice(src: &str, branch: &str, url: &str) -> GithubChoice {
    GithubChoice {
        src: src.into(),
        branch: branch.into(),
        url: url.into(),
    }
}

#[test]
fn github_open_plan_routes_by_choice_count() {
    assert!(matches!(github_open_plan(vec![]), GithubOpenPlan::Continue));
    assert!(matches!(
        github_open_plan(vec![choice("/a", "main", "https://github.com/o/a")]),
        GithubOpenPlan::OpenUrl(url) if url == "https://github.com/o/a"
    ));
    assert!(matches!(
        github_open_plan(vec![
            choice("/a", "main", "https://github.com/o/a"),
            choice("/b", "main", "https://github.com/o/b"),
        ]),
        GithubOpenPlan::Pick(state) if state.choices.len() == 2
    ));
}

#[test]
fn new_selects_first_choice_when_non_empty() {
    let s = GithubPickerState::new(vec![
        choice("/a", "main", "https://github.com/o/a/tree/main"),
        choice("/b", "main", "https://github.com/o/b/tree/main"),
    ]);
    assert_eq!(s.list_state.selected, Some(0));
}

#[test]
fn new_selects_nothing_when_empty() {
    let s = GithubPickerState::new(vec![]);
    assert_eq!(s.list_state.selected, None);
}

#[test]
fn enter_commits_selected_url() {
    // Default selection is index 0 — Enter returns the first URL.
    let mut s = GithubPickerState::new(vec![
        choice("/a", "main", "https://github.com/o/a/tree/main"),
        choice("/b", "dev", "https://github.com/o/b/tree/dev"),
    ]);
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome,
            ModalOutcome::Commit(v) if v == "https://github.com/o/a/tree/main"));
}

#[test]
fn down_then_enter_resolves_second_url() {
    // Pin that Enter commits the URL at the *current* selection, not a
    // stale index.
    let mut s = GithubPickerState::new(vec![
        choice("/a", "main", "https://github.com/o/a/tree/main"),
        choice("/b", "dev", "https://github.com/o/b/tree/dev"),
    ]);
    s.handle_key(key(KeyCode::Down));
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome,
            ModalOutcome::Commit(v) if v == "https://github.com/o/b/tree/dev"));
}

#[test]
fn down_wraps_at_end() {
    let mut s = GithubPickerState::new(vec![
        choice("/a", "main", "https://github.com/o/a/tree/main"),
        choice("/b", "dev", "https://github.com/o/b/tree/dev"),
    ]);
    s.handle_key(key(KeyCode::Down));
    s.handle_key(key(KeyCode::Down));
    assert_eq!(s.list_state.selected, Some(0));
}

#[test]
fn up_wraps_at_start() {
    let mut s = GithubPickerState::new(vec![
        choice("/a", "main", "https://github.com/o/a/tree/main"),
        choice("/b", "dev", "https://github.com/o/b/tree/dev"),
    ]);
    s.handle_key(key(KeyCode::Up));
    assert_eq!(s.list_state.selected, Some(1));
}

#[test]
fn esc_cancels() {
    let mut s = GithubPickerState::new(vec![choice(
        "/a",
        "main",
        "https://github.com/o/a/tree/main",
    )]);
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn enter_on_empty_list_is_continue() {
    let mut s = GithubPickerState::new(vec![]);
    assert!(matches!(
        s.handle_key(key(KeyCode::Enter)),
        ModalOutcome::Continue
    ));
}

fn render_buffer(state: &GithubPickerState, w: u16, h: u16) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render(f, Rect::new(0, 0, w, h), state))
        .unwrap();
    term.backend().buffer().clone()
}

#[test]
fn selected_row_uses_shared_full_width_highlight() {
    let state = GithubPickerState::new(vec![choice(
        "/workspace/repo",
        "main",
        "https://github.com/o/repo/tree/main",
    )]);

    let buffer = render_buffer(&state, 60, 8);
    let selected_y = (0..8)
        .find(|y| buffer[(1, *y)].symbol() == "\u{25b8}")
        .expect("selected row should show shared cursor");
    for x in 1..59 {
        assert_eq!(
            buffer[(x, selected_y)].bg,
            jackin_ui::theme::accent_fg(),
            "x={x}"
        );
    }
    assert_ne!(
        buffer[(59, selected_y)].bg,
        jackin_ui::theme::accent_fg(),
        "selection must not paint the dialog border"
    );
}
