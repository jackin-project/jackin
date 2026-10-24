// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::{Position, Rect},
};

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

fn choices(count: usize) -> WorkdirPickState {
    let choices = (0..count)
        .map(|index| WorkdirChoice {
            path: format!("/w/project-{index:02}"),
            label: format!("choice-{index:02}-painted"),
        })
        .collect::<Vec<_>>();
    WorkdirPickState {
        list_state: ListState::for_count(choices.len()),
        choices,
    }
}

fn render_buffer(state: &WorkdirPickState, terminal_area: Rect, paint_area: Rect) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(terminal_area.width, terminal_area.height))
        .expect("Workdir TestBackend should initialize");
    terminal
        .draw(|frame| render(frame, paint_area, state))
        .expect("Workdir picker should render to TestBackend");
    terminal.backend().buffer().clone()
}

fn painted_position(buffer: &Buffer, area: Rect, marker: &str) -> Option<Position> {
    (area.y..area.bottom()).find_map(|row| {
        let text = (area.x..area.right())
            .map(|column| buffer[(column, row)].symbol())
            .collect::<String>();
        let byte = text.find(marker)?;
        let column = termrock::text::display_cols(&text[..byte]);
        Some(Position::new(
            area.x.saturating_add(
                u16::try_from(column).expect("painted Workdir marker column fits u16"),
            ),
            row,
        ))
    })
}

#[test]
fn single_mount_generates_dst_plus_ancestors_minus_filtered() {
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

#[test]
fn painted_regions_cover_visible_workdir_rows_only() {
    let terminal_area = Rect::new(0, 0, 100, 30);
    let translated_area = Rect::new(7, 4, 60, 14);
    let mut short_list = choices(3);
    short_list.list_state.select(Some(1));
    let buffer = render_buffer(&short_list, terminal_area, translated_area);
    let selected_label = painted_position(&buffer, translated_area, "choice-01-painted")
        .expect("the selected Workdir label appears in the actual TestBackend buffer");
    assert!(selection_hit_at(
        translated_area,
        &short_list,
        selected_label
    ));

    let leading_row_cell = (translated_area.x + 1..selected_label.x)
        .find(|column| buffer[(*column, selected_label.y)].symbol() == " ")
        .map(|column| Position::new(column, selected_label.y))
        .expect("the rendered Workdir row has a blank leading cell");
    assert!(selection_hit_at(
        translated_area,
        &short_list,
        leading_row_cell
    ));
    let label_width =
        u16::try_from("choice-01-painted".len()).expect("painted Workdir label width fits u16");
    let trailing_row_cell = (selected_label.x + label_width..translated_area.right() - 1)
        .find(|column| buffer[(*column, selected_label.y)].symbol() == " ")
        .map(|column| Position::new(column, selected_label.y))
        .expect("the rendered Workdir row has a blank trailing cell");
    assert!(selection_hit_at(
        translated_area,
        &short_list,
        trailing_row_cell
    ));

    let title = painted_position(&buffer, translated_area, "Working directory")
        .expect("the Workdir title appears in the actual TestBackend buffer");
    for unpainted in [
        title,
        Position::new(translated_area.x, translated_area.y),
        Position::new(translated_area.x + 2, translated_area.y + 1),
        Position::new(
            translated_area.x + 2,
            translated_area.y + translated_area.height - 3,
        ),
    ] {
        assert!(
            !selection_hit_at(translated_area, &short_list, unpainted),
            "unpainted Workdir cell {unpainted:?} must not be selectable"
        );
    }

    let empty_list = choices(0);
    let empty_buffer = render_buffer(&empty_list, terminal_area, translated_area);
    let empty_message = painted_position(&empty_buffer, translated_area, "no directories")
        .expect("the empty-state message appears in the actual TestBackend buffer");
    assert!(!selection_hit_at(
        translated_area,
        &empty_list,
        empty_message
    ));
    for tiny_area in [Rect::new(3, 3, 1, 1), Rect::new(3, 3, 0, 0)] {
        assert!(!selection_hit_at(
            tiny_area,
            &short_list,
            Position::new(tiny_area.x, tiny_area.y)
        ));
    }

    let mut long_list = choices(20);
    long_list.list_state.select(Some(19));
    let long_area = Rect::new(5, 3, 70, 9);
    let long_buffer = render_buffer(&long_list, terminal_area, long_area);
    let selected_long_label = painted_position(&long_buffer, long_area, "choice-19-painted")
        .expect("the selected Workdir row remains visible after scrolling");
    assert!(selection_hit_at(long_area, &long_list, selected_long_label));
    assert!(painted_position(&long_buffer, long_area, "choice-00-painted").is_none());
    let scrollbar_cell = (long_area.y..long_area.bottom())
        .flat_map(|row| (long_area.x..long_area.right()).map(move |column| (column, row)))
        .find(|(column, row)| matches!(long_buffer[(*column, *row)].symbol(), "┃" | "█"))
        .map(|(column, row)| Position::new(column, row))
        .expect("the long rendered Workdir list paints a scrollbar thumb");
    assert!(!selection_hit_at(long_area, &long_list, scrollbar_cell));
}
