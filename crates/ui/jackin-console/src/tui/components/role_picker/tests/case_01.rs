// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn new_selects_first_when_non_empty() {
    let s = RolePickerState::new(roles(&["chainargos/agent-smith", "agent-brown"]));
    assert_eq!(s.list_state.selected().copied(), Some(0));
    assert_eq!(s.filtered.len(), 2);
}

#[test]
fn new_selects_nothing_when_empty() {
    let s = RolePickerState::<TestRole>::new(vec![]);
    assert_eq!(s.list_state.selected().copied(), None);
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
    assert_eq!(s.list_state.selected().copied(), Some(0));
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
    assert_eq!(s.list_state.selected().copied(), Some(0));
}

#[test]
fn filter_empty_shows_all() {
    let mut s = RolePickerState::new(roles(&["agent-smith", "agent-brown"]));
    s.handle_key(key(KeyCode::Char('s')));
    assert_eq!(s.filtered.len(), 1);
    s.handle_key(key(KeyCode::Backspace));
    assert!(s.filter.is_empty());
    assert_eq!(s.filtered.len(), 2);
    assert_eq!(s.list_state.selected().copied(), Some(0));
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
    assert_eq!(s.list_state.selected().copied(), Some(0));
}

#[test]
fn up_wraps_at_start() {
    let mut s = RolePickerState::new(roles(&["agent-a", "agent-b"]));
    s.handle_key(key(KeyCode::Up));
    assert_eq!(s.list_state.selected().copied(), Some(1));
}

#[test]
fn j_and_k_append_to_filter_not_navigate() {
    let mut s = RolePickerState::new(roles(&["agent-jenkins", "agent-kafka"]));
    s.handle_key(key(KeyCode::Char('j')));
    assert_eq!(s.filter, "j");
    assert_eq!(s.filtered.len(), 1);
    assert_eq!(s.filtered[0].key(), "agent-jenkins");
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

fn render_role_picker_buffer(
    area: Rect,
    state: &RolePickerState<TestRole>,
    width: u16,
    height: u16,
) -> Buffer {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, area, state)).unwrap();
    terminal.backend().buffer().clone()
}

fn painted_role_text(buffer: &Buffer, area: Rect, expected: &str) -> Option<Position> {
    for row in area.y..area.bottom() {
        let text = (area.x..area.right())
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>();
        if let Some(byte) = text.find(expected) {
            let column = termrock::text::display_cols(&text[..byte]);
            return Some(Position::new(
                area.x.saturating_add(u16::try_from(column).ok()?),
                row,
            ));
        }
    }
    None
}

#[test]
fn role_picker_selection_hits_match_painted_rows() {
    let area = Rect::new(7, 5, 44, 12);
    let state = RolePickerState::new(roles(&["role-00", "role-01"]));
    let buffer = render_role_picker_buffer(area, &state, 80, 30);
    let title = painted_role_text(&buffer, area, "Select Role").expect("painted title");
    let filter = painted_role_text(&buffer, area, "Filter:").expect("painted filter label");
    let first = painted_role_text(&buffer, area, "role-00").expect("painted first role");
    let inner = role_picker_inner_area(area);
    let list_area = role_picker_rows_area(inner);
    let selected_before = state.list_state.selected().copied();
    let filter_before = state.filter.clone();

    assert!(selection_hit_at(area, &state, first));
    let same_row_blank = Position::new(first.x + "role-00".len() as u16, first.y);
    assert_eq!(buffer[(same_row_blank.x, same_row_blank.y)].symbol(), " ");
    assert!(selection_hit_at(area, &state, same_row_blank));
    assert_eq!(state.list_state.selected().copied(), selected_before);
    assert_eq!(state.filter, filter_before);

    for miss in [
        title,
        Position::new(area.x, area.y),
        filter,
        Position::new(list_area.x, list_area.y.saturating_sub(1)),
        Position::new(list_area.x.saturating_add(4), list_area.y + 2),
        Position::new(area.x - 1, area.y),
        Position::new(0, 0),
    ] {
        assert!(
            !selection_hit_at(area, &state, miss),
            "unexpected hit at {miss:?}"
        );
    }
    assert!(!selection_hit_at(
        Rect::new(area.x, area.y, 0, 0),
        &state,
        first
    ));
    assert!(!selection_hit_at(
        Rect::new(area.x, area.y, 4, 4),
        &state,
        first
    ));

    let mut no_matches = RolePickerState::new(roles(&["role-00", "role-01"]));
    no_matches.handle_key(key(KeyCode::Char('z')));
    let no_matches_buffer = render_role_picker_buffer(area, &no_matches, 80, 30);
    let empty_text =
        painted_role_text(&no_matches_buffer, area, "no matches").expect("painted no-match state");
    assert!(!selection_hit_at(area, &no_matches, empty_text));

    let mut resized = RolePickerState::new(roles(&["role-00", "role-01", "role-02"]));
    resized.list_state.select(Some(2));
    let before_filter = render_role_picker_buffer(area, &resized, 80, 30);
    let prior_last = painted_role_text(&before_filter, area, "role-02")
        .expect("painted selected row before filter");
    assert!(selection_hit_at(area, &resized, prior_last));
    for ch in "02".chars() {
        resized.handle_key(key(KeyCode::Char(ch)));
    }
    assert_eq!(resized.filter, "02");
    assert_eq!(resized.filtered.len(), 1);
    assert_eq!(resized.list_state.selected().copied(), Some(0));
    let resized_area = Rect::new(10, 7, 36, 7);
    let resized_buffer = render_role_picker_buffer(resized_area, &resized, 80, 30);
    let current_last = painted_role_text(&resized_buffer, resized_area, "role-02")
        .expect("painted role after filter and resize");
    let resized_list = role_picker_rows_area(role_picker_inner_area(resized_area));
    assert!(selection_hit_at(resized_area, &resized, current_last));
    assert!(!selection_hit_at(
        resized_area,
        &resized,
        Position::new(resized_list.x, resized_list.y + 1)
    ));
    assert_eq!(resized.filter, "02");
    assert_eq!(resized.list_state.selected().copied(), Some(0));

    let long_roles = (0..20)
        .map(|index| format!("role-{index:02}"))
        .collect::<Vec<_>>();
    let long_roles = long_roles.iter().map(String::as_str).collect::<Vec<_>>();
    let mut long = RolePickerState::new(roles(&long_roles));
    long.list_state.select(Some(19));
    let long_buffer = render_role_picker_buffer(area, &long, 80, 30);
    let selected_last = painted_role_text(&long_buffer, area, "role-19")
        .expect("selected last role is painted after scroll");
    let long_list = role_picker_rows_area(role_picker_inner_area(area));
    assert!(selection_hit_at(area, &long, selected_last));
    assert!(!selection_hit_at(
        area,
        &long,
        Position::new(long_list.right() - 1, selected_last.y)
    ));
    assert_eq!(long.list_state.selected().copied(), Some(19));
}
