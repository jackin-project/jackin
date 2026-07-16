// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `role_picker`.
use super::*;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestRole(String);

impl RoleChoice for TestRole {
    fn key(&self) -> String {
        self.0.clone()
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

fn roles(keys: &[&str]) -> Vec<TestRole> {
    keys.iter().map(|k| TestRole((*k).to_owned())).collect()
}

#[test]
fn new_selects_first_when_non_empty() {
    let s = RolePickerState::new(roles(&["chainargos/agent-smith", "agent-brown"]));
    assert_eq!(s.list_state.selected, Some(0));
    assert_eq!(s.filtered.len(), 2);
}

#[test]
fn new_selects_nothing_when_empty() {
    let s = RolePickerState::<TestRole>::new(vec![]);
    assert_eq!(s.list_state.selected, None);
}

#[test]
fn launch_uses_launch_confirm_label() {
    let s = RolePickerState::launch(roles(&["agent-smith"]));

    assert_eq!(s.confirm_label, "launch");
}

#[test]
fn enter_commits_selected_agent() {
    let mut s = RolePickerState::new(roles(&["chainargos/agent-smith", "chainargos/agent-brown"]));
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome,
            ModalOutcome::Commit(a) if a.key() == "chainargos/agent-smith"));
}

#[test]
fn esc_cancels() {
    let mut s = RolePickerState::new(roles(&["agent-smith"]));
    assert!(matches!(
        s.handle_key(key(KeyCode::Esc)),
        ModalOutcome::Cancel
    ));
}

#[test]
fn filter_narrows_agent_list() {
    let mut s = RolePickerState::new(roles(&[
        "chainargos/agent-smith",
        "chainargos/agent-brown",
        "agent-architect",
    ]));
    for ch in "smith".chars() {
        s.handle_key(key(KeyCode::Char(ch)));
    }
    assert_eq!(s.filter, "smith");
    assert_eq!(s.filtered.len(), 1);
    assert_eq!(s.filtered[0].key(), "chainargos/agent-smith");
    assert_eq!(s.list_state.selected, Some(0));
}

#[test]
fn filter_shrinking_below_selection_resets_to_first_match() {
    let mut s = RolePickerState::new(roles(&[
        "chainargos/agent-smith",
        "chainargos/agent-brown",
        "agent-architect",
    ]));
    s.list_state.select(Some(2));

    for ch in "brown".chars() {
        s.handle_key(key(KeyCode::Char(ch)));
    }

    assert_eq!(s.filtered.len(), 1);
    assert_eq!(s.filtered[0].key(), "chainargos/agent-brown");
    assert_eq!(s.list_state.selected, Some(0));
}

#[test]
fn filter_empty_shows_all() {
    let mut s = RolePickerState::new(roles(&["agent-smith", "agent-brown"]));
    s.handle_key(key(KeyCode::Char('s')));
    assert_eq!(s.filtered.len(), 1);
    s.handle_key(key(KeyCode::Backspace));
    assert!(s.filter.is_empty());
    assert_eq!(s.filtered.len(), 2);
    assert_eq!(s.list_state.selected, Some(0));
}

#[test]
fn enter_on_empty_filtered_list_is_noop() {
    let mut s = RolePickerState::new(roles(&["agent-smith"]));
    for ch in "zzzz".chars() {
        s.handle_key(key(KeyCode::Char(ch)));
    }
    assert!(s.filtered.is_empty());
    let outcome = s.handle_key(key(KeyCode::Enter));
    assert!(matches!(outcome, ModalOutcome::Continue));
}

#[test]
fn down_wraps_at_end() {
    let mut s = RolePickerState::new(roles(&["agent-a", "agent-b"]));
    s.handle_key(key(KeyCode::Down));
    s.handle_key(key(KeyCode::Down));
    assert_eq!(s.list_state.selected, Some(0));
}

#[test]
fn up_wraps_at_start() {
    let mut s = RolePickerState::new(roles(&["agent-a", "agent-b"]));
    s.handle_key(key(KeyCode::Up));
    assert_eq!(s.list_state.selected, Some(1));
}

/// `j`/`k` append to the filter (no vim-style nav) so roles with
/// those letters in their key can be typed naturally.
#[test]
fn j_and_k_append_to_filter_not_navigate() {
    let mut s = RolePickerState::new(roles(&["agent-jenkins", "agent-kafka"]));
    s.handle_key(key(KeyCode::Char('j')));
    assert_eq!(s.filter, "j");
    assert_eq!(s.filtered.len(), 1);
    assert_eq!(s.filtered[0].key(), "agent-jenkins");
}

// ── Render-buffer smoke tests ─────────────────────────────────────

fn dump(state: &RolePickerState<TestRole>, w: u16, h: u16) -> String {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| {
        let area = Rect::new(0, 0, w, h);
        render(f, area, state);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn agent_picker_renders_filter_row_with_placeholder_dots_when_empty() {
    let s = RolePickerState::new(roles(&["chainargos/agent-smith"]));
    let frame = dump(&s, 60, 12);
    assert!(
        frame.contains("Filter:"),
        "filter row label missing; frame:\n{frame}"
    );
    assert!(
        frame.contains('\u{2591}'),
        "filter row missing placeholder dots `░`; frame:\n{frame}"
    );
    let top: String = frame.lines().next().unwrap().to_owned();
    assert!(
        top.contains("Select Role"),
        "title bar must read `Select Role`; top row:\n{top}"
    );
    assert!(
        !top.contains("filter:"),
        "filter must NOT be inlined into the title; top row:\n{top}"
    );
}

#[test]
fn agent_picker_renders_filter_row_with_live_chars_when_typing() {
    let mut s = RolePickerState::new(roles(&["chainargos/agent-smith", "chainargos/agent-brown"]));
    for ch in "smi".chars() {
        s.handle_key(key(KeyCode::Char(ch)));
    }
    let frame = dump(&s, 60, 12);
    assert!(
        frame.contains("Filter: smi"),
        "filter row must show live characters; frame:\n{frame}"
    );
    let top: String = frame.lines().next().unwrap().to_owned();
    assert!(
        !top.contains("smi"),
        "live filter must NOT bleed into the title; top row:\n{top}"
    );
}

#[test]
fn agent_picker_renders_no_empty_state_placeholder_when_filter_excludes_all() {
    let mut s = RolePickerState::new(roles(&["agent-smith", "agent-brown"]));
    for ch in "zzzz".chars() {
        s.handle_key(key(KeyCode::Char(ch)));
    }
    assert!(s.filtered.is_empty());
    let frame = dump(&s, 60, 12);
    assert!(
        !frame.contains("(no roles match"),
        "must not render an empty-state placeholder; frame:\n{frame}"
    );
    assert!(
        !frame.contains("(no items match"),
        "must not render an empty-state placeholder; frame:\n{frame}"
    );
    assert!(frame.contains("Filter: zzzz"));
}

fn render_buffer(state: &RolePickerState<TestRole>, w: u16, h: u16) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let backend = TestBackend::new(w, h);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render(f, Rect::new(0, 0, w, h), state))
        .unwrap();
    term.backend().buffer().clone()
}

#[test]
fn selected_row_uses_shared_full_width_highlight() {
    let state = RolePickerState::new(roles(&["chainargos/agent-smith"]));

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
