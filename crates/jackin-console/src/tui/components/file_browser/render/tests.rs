// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `render`.
use super::*;
use std::path::PathBuf;
use tempfile::tempdir;

fn make_state_at(path: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        path.clone(),
        path,
    ))
}

fn row_string(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn char_column(row: &str, needle: &str) -> usize {
    let byte = row
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} should appear in {row:?}"));
    row[..byte].chars().count()
}

// ── Render: ensure the ` (git)` suffix actually appears ───────────

#[test]
fn git_entries_render_with_git_suffix() {
    use ratatui::{Terminal, backend::TestBackend};

    let tmp = tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    std::fs::create_dir(tmp.path().join("plain")).unwrap();

    // Use a state where the selection is NOT on the git row, so the
    // suffix renders as a separate span rather than getting absorbed
    // into the highlight style.
    let mut state = make_state_at(tmp.path().to_path_buf());
    // Sort order is alphabetical lowercase: plain < repo. Select plain
    // (index 0) so repo's ` (git)` suffix renders unhighlighted.
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
    // Locate the first cell of the name "plain" — rows start at y=0
    // with the block's top border, so the first entry sits at y=1
    // and the name begins at x = 1 (border) + 2 (indent) = 3.
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
    // Clear selection so the highlight style doesn't mask the spans.
    state.list_state.select(None);

    let backend = TestBackend::new(40, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(frame, frame.area(), &state);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    // First entry row is at y=1 (below the block's top border).
    // Name starts at x = 1 (border) + 2 (indent) = 3.
    let name_cell = &buffer[(3, 1)];
    assert_eq!(
        name_cell.symbol(),
        "r",
        "expected 'r' at name's first char, got {:?}",
        name_cell.symbol()
    );
    // Suffix: "  repo/ (git)" — the '(' of "(git)" sits at
    // x = 3 (name start) + 5 (len of "repo/") + 1 (space) = 9.
    let paren_cell = &buffer[(9, 1)];
    assert_eq!(
        paren_cell.symbol(),
        "(",
        "expected '(' at the suffix's first char, got {:?}",
        paren_cell.symbol()
    );
}
