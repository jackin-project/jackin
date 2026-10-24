// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::github_mounts::GithubChoice;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{Terminal, backend::TestBackend, layout::Position};

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

fn render_buffer(state: &GithubPickerState, terminal_area: Rect, paint_area: Rect) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("GitHub picker TestBackend should initialize");
    terminal
        .draw(|frame| render(frame, paint_area, state))
        .expect("GitHub picker should render to TestBackend");
    terminal.backend().buffer().clone()
}

fn painted_position(buffer: &Buffer, area: Rect, needle: &str) -> Option<Position> {
    (area.y..area.bottom()).find_map(|row| {
        let text = (area.x..area.right())
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>();
        let byte = text.find(needle)?;
        let column = termrock::text::display_cols(&text[..byte]);
        Some(Position::new(
            area.x.saturating_add(
                u16::try_from(column).expect("painted GitHub label column fits u16"),
            ),
            row,
        ))
    })
}

fn choices(count: usize) -> Vec<GithubChoice> {
    (0..count)
        .map(|index| {
            choice(
                &format!("/workspace/repository-{index:02}"),
                &format!("branch-{index:02}-painted"),
                &format!("https://github.invalid/owner/repository-{index:02}"),
            )
        })
        .collect()
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
    assert_eq!(s.list_state.selected().copied(), Some(0));
}

#[test]
fn new_selects_nothing_when_empty() {
    let s = GithubPickerState::new(vec![]);
    assert_eq!(s.list_state.selected().copied(), None);
}

#[test]
fn enter_commits_selected_url() {
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
    assert_eq!(s.list_state.selected().copied(), Some(0));
}

#[test]
fn up_wraps_at_start() {
    let mut s = GithubPickerState::new(vec![
        choice("/a", "main", "https://github.com/o/a/tree/main"),
        choice("/b", "dev", "https://github.com/o/b/tree/dev"),
    ]);
    s.handle_key(key(KeyCode::Up));
    assert_eq!(s.list_state.selected().copied(), Some(1));
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

#[test]
fn painted_regions_cover_visible_github_rows_only() {
    let terminal_area = Rect::new(0, 0, 80, 30);
    let translated_area = Rect::new(7, 4, 60, 14);
    let state = GithubPickerState::new(choices(3));
    let buffer = render_buffer(&state, terminal_area, translated_area);
    let painted_row = painted_position(&buffer, translated_area, "branch-01-painted")
        .expect("the rendered TestBackend buffer contains the unique GitHub row");
    assert!(selection_hit_at(translated_area, &state, painted_row));

    let leading_row_cell = (translated_area.x + 1..painted_row.x)
        .find(|column| buffer[(*column, painted_row.y)].symbol() == " ")
        .map(|column| Position::new(column, painted_row.y))
        .expect("the rendered row has a blank leading list cell");
    assert!(selection_hit_at(translated_area, &state, leading_row_cell));

    for unpainted in [
        Position::new(translated_area.x + 2, translated_area.y),
        Position::new(translated_area.x, translated_area.y + 2),
        Position::new(translated_area.x + 1, translated_area.y + 1),
        Position::new(translated_area.x + 2, painted_row.y + 4),
    ] {
        assert!(
            !selection_hit_at(translated_area, &state, unpainted),
            "unpainted GitHub dialog cell {unpainted:?} must not be selectable"
        );
    }

    let empty = GithubPickerState::new(Vec::new());
    let empty_buffer = render_buffer(&empty, terminal_area, translated_area);
    let empty_message = painted_position(&empty_buffer, translated_area, "no GitHub sources")
        .expect("the empty-state message is painted");
    assert!(!selection_hit_at(translated_area, &empty, empty_message));

    for tiny_area in [Rect::new(3, 3, 1, 1), Rect::new(3, 3, 0, 0)] {
        assert!(!selection_hit_at(tiny_area, &state, Position::new(3, 3)));
    }

    let mut long_list = GithubPickerState::new(choices(20));
    long_list.list_state.select(Some(19));
    let long_area = Rect::new(5, 3, 70, 9);
    let long_buffer = render_buffer(&long_list, terminal_area, long_area);
    let selected_row = painted_position(&long_buffer, long_area, "branch-19-painted")
        .expect("the selected row remains visible after list scrolling");
    assert!(selection_hit_at(long_area, &long_list, selected_row));
    assert!(
        painted_position(&long_buffer, long_area, "branch-00-painted").is_none(),
        "an offscreen choice must not have a painted row"
    );
    let scrollbar_cell = (long_area.y..long_area.bottom())
        .flat_map(|row| (long_area.x..long_area.right()).map(move |column| (column, row)))
        .find(|(column, row)| matches!(long_buffer[(*column, *row)].symbol(), "┃" | "█"))
        .map(|(column, row)| Position::new(column, row))
        .expect("the long rendered list paints a scrollbar thumb");
    assert!(!selection_hit_at(long_area, &long_list, scrollbar_cell));
}
