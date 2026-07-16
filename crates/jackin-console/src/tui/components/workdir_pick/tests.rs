// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `workdir_pick`.
use super::*;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

struct TestMount {
    dst: String,
}

impl WorkdirMount for TestMount {
    fn dst(&self) -> &str {
        &self.dst
    }
}

fn mount(_src: &str, dst: &str) -> TestMount {
    TestMount { dst: dst.into() }
}

#[test]
fn single_mount_generates_dst_plus_ancestors_minus_filtered() {
    // Intermediate ancestors are kept; `/` and the $HOME-parent are
    // always filtered out regardless of host OS.
    let mounts = vec![mount("/opt/jackin/p", "/opt/jackin/p")];
    let s = WorkdirPickState::from_mounts(&mounts);
    let paths: Vec<&str> = s.choices.iter().map(|c| c.path.as_str()).collect();
    assert!(paths.contains(&"/opt/jackin/p"));
    assert!(paths.contains(&"/opt/jackin"));
    assert!(paths.contains(&"/opt"));
    assert!(!paths.contains(&"/"), "`/` must always be filtered");
}

#[test]
fn first_choice_is_dst_with_mount_dst_label() {
    let mounts = vec![mount("/opt/app", "/opt/app")];
    let s = WorkdirPickState::from_mounts(&mounts);
    assert_eq!(s.choices[0].label, "(mount dst)");
}

#[test]
fn root_path_is_filtered_out() {
    let mounts = vec![mount("/opt/app", "/opt/app")];
    let s = WorkdirPickState::from_mounts(&mounts);
    assert!(
        s.choices.iter().all(|c| c.path != "/"),
        "`/` must be filtered out of the choice list: {:?}",
        s.choices
            .iter()
            .map(|c| c.path.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn home_parent_is_filtered_out() {
    // Build a mount whose dst walks through the user's $HOME so the
    // ancestor chain includes the $HOME-parent directory — which must
    // be filtered.
    let home = directories::BaseDirs::new().map_or_else(
        || "/home/test".to_owned(),
        |b| b.home_dir().display().to_string(),
    );
    let dst = format!("{home}/Projects/app");
    let mounts = vec![mount(&dst, &dst)];
    let s = WorkdirPickState::from_mounts(&mounts);

    let home_parent = std::path::Path::new(&home)
        .parent()
        .map_or_else(|| "/Users".to_owned(), |p| p.display().to_string());

    assert!(
        s.choices.iter().all(|c| c.path != home_parent),
        "home-parent `{home_parent}` must be filtered out of the choice list: {:?}",
        s.choices
            .iter()
            .map(|c| c.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        s.choices.iter().all(|c| c.path != "/"),
        "`/` must also be filtered out"
    );
}

#[test]
fn home_itself_is_labelled_home_not_parent() {
    let home = directories::BaseDirs::new().map_or_else(
        || "/home/test".to_owned(),
        |b| b.home_dir().display().to_string(),
    );
    let dst = format!("{home}/Projects/app");
    let mounts = vec![mount(&dst, &dst)];
    let s = WorkdirPickState::from_mounts(&mounts);

    let home_choice = s
        .choices
        .iter()
        .find(|c| c.path == home)
        .expect("home should appear in ancestor chain");
    assert_eq!(home_choice.label, "(home)");
}

#[test]
fn enter_commits_selected_path() {
    let mounts = vec![mount("/opt/app", "/opt/app")];
    let mut s = WorkdirPickState::from_mounts(&mounts);
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/opt/app"));
}

#[test]
fn down_then_enter_picks_second_choice() {
    let mounts = vec![mount("/opt/app/sub", "/opt/app/sub")];
    let mut s = WorkdirPickState::from_mounts(&mounts);
    s.handle_key(key(KeyCode::Down));
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome, ModalOutcome::Commit(v) if v == "/opt/app"));
}

#[test]
fn duplicate_ancestors_across_mounts_are_deduped() {
    let mounts = vec![mount("/opt/a/b", "/opt/a/b"), mount("/opt/a/c", "/opt/a/c")];
    let s = WorkdirPickState::from_mounts(&mounts);
    let a_count = s.choices.iter().filter(|c| c.path == "/opt/a").count();
    assert_eq!(a_count, 1);
}

#[test]
fn esc_cancels() {
    let mounts = vec![mount("/a", "/a")];
    let mut s = WorkdirPickState::from_mounts(&mounts);
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

fn render_buffer(state: &WorkdirPickState, w: u16, h: u16) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render(f, Rect::new(0, 0, w, h), state))
        .unwrap();
    term.backend().buffer().clone()
}

#[test]
fn selected_row_uses_shared_full_width_highlight() {
    let mounts = vec![mount("/opt/app", "/opt/app")];
    let state = WorkdirPickState::from_mounts(&mounts);

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
