// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::{Position, Rect},
};

fn visible_label_position(buffer: &Buffer, area: Rect, label: &str) -> Option<Position> {
    (area.y..area.bottom()).find_map(|y| {
        let row = row_string(buffer, y);
        row.contains(label).then(|| {
            Position::new(
                u16::try_from(char_column(&row, label)).expect("visible column fits u16"),
                y,
            )
        })
    })
}

fn painted_id_at(state: &FileBrowserState, area: Rect, position: Position) -> Option<usize> {
    selection_region_at(area, state, position)
}

fn assert_no_painted_ids(state: &FileBrowserState, modal_area: Rect, cells: Rect) {
    for row in cells.y..cells.bottom() {
        for column in cells.x..cells.right() {
            assert_eq!(
                painted_id_at(state, modal_area, Position::new(column, row)),
                None,
                "unexpected item target at ({column}, {row}) inside {cells:?}"
            );
        }
    }
}

fn assert_listing_chrome_and_blank_rows_are_inert(
    state: &FileBrowserState,
    area: Rect,
    inner: Rect,
    visible_rows: usize,
) {
    assert_no_painted_ids(state, area, Rect::new(area.x, area.y, area.width, 1));
    assert_no_painted_ids(
        state,
        area,
        Rect::new(area.x, area.bottom() - 1, area.width, 1),
    );
    assert_no_painted_ids(state, area, Rect::new(area.x, area.y, 1, area.height));
    assert_no_painted_ids(
        state,
        area,
        Rect::new(area.right() - 1, area.y, 1, area.height),
    );
    for row in inner.y + u16::try_from(visible_rows).unwrap()..inner.bottom() {
        assert_no_painted_ids(state, area, Rect::new(inner.x, row, inner.width, 1));
    }
    assert_no_painted_ids(state, area, Rect::new(area.x - 1, area.y, 1, area.height));
    assert_no_painted_ids(state, area, Rect::new(area.right(), area.y, 1, area.height));
}

fn assert_scrolled_selected_row_is_visible_and_targetable(
    terminal: &mut Terminal<TestBackend>,
    state: &FileBrowserState,
    area: Rect,
    selected_id: usize,
) {
    terminal.draw(|frame| render(frame, area, state)).unwrap();
    let buffer = terminal.backend().buffer();
    let selected = visible_label_position(buffer, area, "item-11/")
        .expect("selected last entry is revealed in the actual buffer");
    let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
    assert_eq!(painted_id_at(state, area, selected), Some(selected_id));
    for column in inner.x..inner.right() {
        assert_eq!(
            painted_id_at(state, area, Position::new(column, selected.y)),
            Some(selected_id),
            "selected row padding stays targetable"
        );
    }
    assert_eq!(
        painted_id_at(state, area, Position::new(area.right() - 1, selected.y)),
        None,
        "scrollbar gutter is not an item region"
    );
}

#[test]
fn git_entries_render_with_git_suffix() {
    use ratatui::{Terminal, backend::TestBackend};

    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir(tmp.path().join("plain")).unwrap();

    let mut state = make_state_at(tmp.path().to_path_buf());
    state.list_state.select(Some(0));

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let dump = buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(dump.contains("repo/"), "repo row should render: {dump:?}");
    assert!(
        dump.contains("(git)"),
        "git suffix should render on the repo row: {dump:?}"
    );
    assert!(dump.contains("plain/"));
}

#[test]
fn non_git_entry_renders_its_product_name() {
    use ratatui::{Terminal, backend::TestBackend};

    let tmp = tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("plain")).unwrap();

    let state = make_state_at(tmp.path().to_path_buf());
    let mut state = state;
    state.list_state.select(None);

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let cell = &buffer[(3, 1)];
    assert_eq!(
        cell.symbol(),
        "p",
        "expected 'p' at the entry's first char, got {:?}",
        cell.symbol()
    );
}

#[test]
fn overflowing_listing_keeps_selected_product_entry_visible() {
    use ratatui::{Terminal, backend::TestBackend};

    let tmp = tempdir().unwrap();
    for i in 0..8 {
        std::fs::create_dir(tmp.path().join(format!("dir-{i}"))).unwrap();
    }

    let mut state = make_state_at(tmp.path().to_path_buf());
    state.list_state.select(Some(6));

    let backend = TestBackend::new(40, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let dump = buffer
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect::<String>();
    assert!(dump.contains("dir-6/"), "selected entry missing: {dump:?}");
}

#[test]
fn git_prompt_background_suppresses_browser_cursor_and_active_border() {
    use ratatui::{Terminal, backend::TestBackend};

    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = make_state_at(tmp.path().to_path_buf());
    state.list_state.select(Some(0));

    let backend = TestBackend::new(60, 12);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();
    let active_row = row_string(terminal.backend().buffer(), 1);
    let active_repo_col = char_column(&active_row, "repo/");

    state.pending_git_prompt = Some(repo);

    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let background_row = row_string(buffer, 1);
    let background_repo_col = char_column(&background_row, "repo/");
    assert_eq!(
        background_repo_col, active_repo_col,
        "hiding the parent cursor must not move row text: active={active_row:?}, background={background_row:?}"
    );
}

#[test]
fn git_prompt_uses_five_slot_dialog_padding() {
    use ratatui::{Terminal, backend::TestBackend};

    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = make_state_at(tmp.path().to_path_buf());
    state.list_state.select(Some(0));
    state.pending_git_prompt = Some(repo);

    let backend = TestBackend::new(80, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();

    let listing = listing_rect(terminal.backend().buffer().area, false);
    let prompt_rect = crate::tui::components::file_browser::git_prompt_rect(listing, false)
        .expect("git prompt rect");
    let buffer = terminal.backend().buffer();

    let leading = row_string(buffer, prompt_rect.y + 1);
    assert!(
        !leading.contains("What would you like to do?"),
        "leading spacer must be blank before prompt row: {leading:?}"
    );
    let prompt = row_string(buffer, prompt_rect.y + 2);
    assert!(
        prompt.contains("What would you like to do?"),
        "prompt must render after leading spacer: {prompt:?}"
    );
    let spacer = row_string(buffer, prompt_rect.y + 3);
    assert!(
        !spacer.contains("Mount this repository"),
        "content/action spacer must be blank: {spacer:?}"
    );
    let buttons = row_string(buffer, prompt_rect.y + 4);
    assert!(
        buttons.contains("Mount this repository"),
        "buttons must render in action row: {buttons:?}"
    );
}

#[test]
fn git_entry_renders_product_name_and_marker() {
    use ratatui::{Terminal, backend::TestBackend};

    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let mut state = make_state_at(tmp.path().to_path_buf());
    state.list_state.select(None);

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let name_cell = &buffer[(3, 1)];
    assert_eq!(
        name_cell.symbol(),
        "r",
        "expected 'r' at name's first char, got {:?}",
        name_cell.symbol()
    );
    let paren_cell = &buffer[(9, 1)];
    assert_eq!(
        paren_cell.symbol(),
        "(",
        "expected '(' at the suffix's first char, got {:?}",
        paren_cell.symbol()
    );
}

#[test]
fn painted_listing_regions_match_normal_and_scrolled_buffer() {
    let tmp = tempdir().unwrap();
    for name in ["alpha", "beta", "λambda"] {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
    }

    let mut state = make_state_at(tmp.path().to_path_buf());
    let expected_names = ["alpha", "beta", "λambda"];
    let expected_ids: Vec<_> = expected_names
        .iter()
        .map(|name| {
            state
                .entries
                .iter()
                .position(|entry| entry.name == *name)
                .unwrap_or_else(|| panic!("expected listing entry {name:?}: {:?}", state.entries))
        })
        .collect();
    state.list_state.select(Some(expected_ids[2]));

    let area = Rect::new(5, 3, 40, 8);
    let mut terminal = Terminal::new(TestBackend::new(56, 16)).unwrap();
    terminal.draw(|frame| render(frame, area, &state)).unwrap();

    {
        let buffer = terminal.backend().buffer();
        let inner = Rect::new(area.x + 1, area.y + 1, area.width - 2, area.height - 2);
        for (name, id) in expected_names.iter().zip(expected_ids.iter().copied()) {
            let label = format!("{name}/");
            let position = visible_label_position(buffer, area, &label)
                .unwrap_or_else(|| panic!("actual FileBrowser buffer lacks {label:?}"));
            assert_eq!(painted_id_at(&state, area, position), Some(id), "{label}");
            for column in inner.x..inner.right() {
                assert_eq!(
                    painted_id_at(&state, area, Position::new(column, position.y)),
                    Some(id),
                    "the complete painted row, including leading and trailing padding, targets {label}"
                );
            }
        }

        let selected_label = "λambda/";
        let selected = visible_label_position(buffer, area, selected_label).unwrap();
        assert_eq!(state.list_state.selected().copied(), Some(expected_ids[2]));
        assert_eq!(buffer[(inner.x, selected.y)].symbol(), "▌");

        assert_listing_chrome_and_blank_rows_are_inert(&state, area, inner, expected_names.len());
    }

    let overflow_root = tmp.path().join("overflow");
    std::fs::create_dir(&overflow_root).unwrap();
    for index in 0..12 {
        std::fs::create_dir(overflow_root.join(format!("item-{index:02}"))).unwrap();
    }
    let mut overflow = make_state_at(overflow_root);
    let selected_id = overflow
        .entries
        .iter()
        .position(|entry| entry.name == "item-11")
        .expect("last entry exists");
    overflow.list_state.select(Some(selected_id));
    let overflow_area = Rect::new(5, 3, 40, 7);
    assert_scrolled_selected_row_is_visible_and_targetable(
        &mut terminal,
        &overflow,
        overflow_area,
        selected_id,
    );
}

#[test]
fn painted_listing_regions_exclude_chrome_banner_and_blank_rows() {
    let tmp = tempdir().unwrap();
    for name in ["alpha", "beta"] {
        std::fs::create_dir(tmp.path().join(name)).unwrap();
    }
    let mut state = make_state_at(tmp.path().to_path_buf());
    state.rejected_reason = Some("permission denied".into());

    let area = Rect::new(4, 2, 40, 11);
    let mut terminal = Terminal::new(TestBackend::new(52, 18)).unwrap();
    terminal.draw(|frame| render(frame, area, &state)).unwrap();

    let buffer = terminal.backend().buffer();
    let banner = row_string(buffer, area.y);
    assert!(banner.contains("permission denied"), "banner: {banner:?}");
    assert_eq!(
        listing_rect(area, true).y,
        area.y + 1,
        "the rejection row is removed before list geometry"
    );

    let listing = listing_rect(area, true);
    let inner = Rect::new(
        listing.x + 1,
        listing.y + 1,
        listing.width - 2,
        listing.height - 2,
    );
    let id = state
        .entries
        .iter()
        .position(|entry| entry.name == "alpha")
        .expect("alpha entry exists");
    let alpha = visible_label_position(buffer, listing, "alpha/").expect("alpha is painted");
    assert_eq!(painted_id_at(&state, area, alpha), Some(id));
    for column in inner.x..inner.right() {
        assert_eq!(
            painted_id_at(&state, area, Position::new(column, alpha.y)),
            Some(id)
        );
    }
    assert_no_painted_ids(&state, area, Rect::new(area.x, area.y, area.width, 1));
    assert_no_painted_ids(
        &state,
        area,
        Rect::new(listing.x, listing.y, listing.width, 1),
    );
    assert_no_painted_ids(
        &state,
        area,
        Rect::new(listing.x, listing.bottom() - 1, listing.width, 1),
    );
    assert_no_painted_ids(
        &state,
        area,
        Rect::new(listing.x, listing.y, 1, listing.height),
    );
    assert_no_painted_ids(
        &state,
        area,
        Rect::new(listing.right() - 1, listing.y, 1, listing.height),
    );
    for row in inner.y + 2..inner.bottom() {
        assert_no_painted_ids(&state, area, Rect::new(inner.x, row, inner.width, 1));
    }
}

#[test]
fn painted_listing_regions_handle_tiny_translated_viewports() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("entry")).unwrap();
    let state = make_state_at(tmp.path().to_path_buf());
    let areas = [
        Rect::new(11, 6, 0, 0),
        Rect::new(11, 6, 0, 4),
        Rect::new(11, 6, 4, 0),
        Rect::new(11, 6, 1, 1),
        Rect::new(11, 6, 2, 2),
        Rect::new(11, 6, 2, 5),
        Rect::new(11, 6, 5, 2),
    ];

    for area in areas {
        assert_eq!(
            painted_id_at(&state, area, Position::new(area.x, area.y)),
            None,
            "tiny translated area {area:?} has no item target"
        );
    }

    let empty_root = tmp.path().join("empty");
    std::fs::create_dir(&empty_root).unwrap();
    let empty = make_state_at(empty_root);
    let normal_area = Rect::new(11, 6, 32, 8);
    assert!(empty.entries.is_empty());
    assert_eq!(
        painted_id_at(
            &empty,
            normal_area,
            Position::new(normal_area.x + 2, normal_area.y + 2)
        ),
        None,
        "an empty listing has no painted item target"
    );

    let mut terminal = Terminal::new(TestBackend::new(30, 14)).unwrap();
    for area in areas {
        terminal.draw(|frame| render(frame, area, &state)).unwrap();
    }
}
