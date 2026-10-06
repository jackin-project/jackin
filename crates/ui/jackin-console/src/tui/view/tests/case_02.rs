// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn list_vertical_clamp_uses_rendered_sidebar_height() {
    use crate::tui::layout::list::{clamp_list_scroll_for_area, selected_sidebar_scroll_areas};
    use crate::tui::state::ManagerState;
    use jackin_config::{AppConfig, MountConfig, MountIsolation, WorkspaceConfig};
    use ratatui::layout::Rect;
    use termrock::scroll::{
        max_offset_u16 as max_scroll_offset, viewport_height as scroll_viewport_height,
    };

    fn split_mount(idx: usize) -> MountConfig {
        MountConfig {
            src: format!("/host/long/source/path/{idx}"),
            dst: format!("/container/long/destination/path/{idx}"),
            readonly: false,
            isolation: MountIsolation::Shared,
        }
    }

    let mut config = AppConfig::default();
    config.workspaces.insert(
        "demo".into(),
        WorkspaceConfig {
            workdir: "/workspace/demo".into(),
            mounts: (0..10).map(split_mount).collect(),
            ..Default::default()
        },
    );
    let tmp = tempfile::tempdir().unwrap();
    let mut state = ManagerState::from_config(&config, tmp.path());
    state.selected = 1;

    let body = Rect::new(0, 0, 100, 10);
    let columns = crate::tui::list_geometry::split_list_columns(body, state.list_split_pct);
    let areas =
        selected_sidebar_scroll_areas(columns.preview, &state, &config, tmp.path()).unwrap();
    let rendered_viewport = scroll_viewport_height(areas.workspace.area);
    let desired_viewport = scroll_viewport_height(Rect::new(0, 0, 0, 12));
    assert!(rendered_viewport < desired_viewport);

    let expected = max_scroll_offset(areas.workspace.content_height, rendered_viewport);
    assert!(expected > max_scroll_offset(areas.workspace.content_height, desired_viewport));

    crate::tui::scroll_block::scroll_area_set_y(&mut state.list_mounts_scroll, u16::MAX);
    clamp_list_scroll_for_area(body, &mut state, &config, tmp.path());

    assert_eq!(state.list_mounts_scroll.offset_y(), expected);
}

#[test]
fn tui_header_uses_canonical_brand_wordmark() {
    use ratatui::layout::Rect;

    let backend = TestBackend::new(40, 1);
    let mut term = Terminal::new(backend).unwrap();
    term.draw(|f| render_header(f, Rect::new(0, 0, 40, 1), "workspaces"))
        .unwrap();

    let buf = term.backend().buffer();
    let dump: String = buf
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();

    assert!(
        dump.contains("jackin❯"),
        "header must render 'jackin❯' (lowercase + chevron wordmark); got {dump:?}"
    );
    assert!(
        !dump.contains("JACKIN"),
        "header must not render 'JACKIN' (uppercase); got {dump:?}"
    );
}

#[test]
fn dialog_button_rows_have_one_blank_row_above() {
    for (name, (buf, _area), labels) in [
        (
            "SaveDiscardCancel",
            render_save_discard(),
            &["Save", "Discard", "Cancel"][..],
        ),
        ("Confirm", render_confirm(), &["Yes", "No"][..]),
        (
            "MountDstChoice",
            render_mount_dst(),
            &["Mount at same path", "Edit destination", "Cancel"][..],
        ),
        (
            "ConfirmSave",
            render_confirm_save(),
            &["Save", "Cancel"][..],
        ),
    ] {
        let button_y = button_row_y(&buf, labels);
        assert!(
            button_y > buf.area.y,
            "{name} button row cannot be first row"
        );
        let before = row_text(&buf, button_y - 1);
        let non_space_cells = before.chars().filter(|ch| !ch.is_whitespace()).count();
        assert!(
            non_space_cells <= 2,
            "{name} must have one blank row above buttons; got {before:?}",
        );
    }
}

#[test]
fn snapshot_list_empty_80x24() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let rendered = render_manager_state(&mut state, &config, &cwd, 80, 24);
    insta::assert_snapshot!("list_empty_80x24", rendered);
}

#[test]
fn new_workspace_hints_stay_in_footer() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 1;

    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 24);

    assert!(
        !rendered.contains("Press Enter"),
        "new-workspace body must not render keyboard hints inline:\n{rendered}"
    );
    assert!(
        rendered.contains("setup"),
        "footer must own the Enter/setup hint:\n{rendered}"
    );
}

#[test]
fn snapshot_settings_general_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.stage = ManagerStage::Settings(SettingsState::from_config(&config));
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("settings_general_90x20", rendered);
}

#[test]
fn snapshot_editor_general_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.stage = ManagerStage::Editor(EditorState::new_edit(
        "my-workspace".into(),
        WorkspaceConfig::default(),
    ));
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("editor_general_90x20", rendered);
}

#[test]
fn editor_general_content_focus_shows_cursor() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("my-workspace".into(), WorkspaceConfig::default());
    editor.set_tab_bar_focused(false);
    editor.set_tab_content_scroll_focused(true);
    state.stage = ManagerStage::Editor(editor);

    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);

    assert!(
        rendered.contains("▸ Name"),
        "focused General tab must show the same cursor signal as its green border:\n{rendered}"
    );
}

#[test]
fn snapshot_editor_mounts_tab_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;
    state.stage = ManagerStage::Editor(editor);
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("editor_mounts_tab_90x20", rendered);
}

#[test]
fn host_console_content_states_project_visible_focus() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut cases: Vec<(&str, ManagerState<'_>)> = Vec::new();

    let mut list = ManagerState::from_config(&config, &cwd);
    list.set_list_names_focused(true);
    cases.push(("list", list));

    for (name, tab) in [
        ("editor general", EditorTab::General),
        ("editor mounts", EditorTab::Mounts),
        ("editor roles", EditorTab::Roles),
        ("editor secrets", EditorTab::Secrets),
        ("editor auth", EditorTab::Auth),
    ] {
        let mut state = ManagerState::from_config(&config, &cwd);
        let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
        editor.active_tab = tab;
        editor.set_tab_bar_focused(false);
        editor.set_tab_content_scroll_focused(true);
        editor.set_workspace_mounts_scroll_focused(tab == EditorTab::Mounts);
        state.stage = ManagerStage::Editor(editor);
        cases.push((name, state));
    }

    for tab in crate::tui::state::SettingsTab::ALL {
        let mut state = ManagerState::from_config(&config, &cwd);
        let mut settings = SettingsState::from_config(&config);
        settings.active_tab = tab;
        settings.set_active_content_focused(true);
        state.stage = ManagerStage::Settings(settings);
        cases.push((tab.label(), state));
    }

    for (name, mut state) in cases {
        let buf = render_manager_buffer(&mut state, &config, &cwd, 100, 28);
        assert!(
            focused_region_count(&buf) >= 1,
            "{name} must project focused state into the rendered composition"
        );
    }
}

#[test]
fn host_console_list_detail_transitions_project_visible_focus() {
    let cwd = test_cwd();

    let mut cases: Vec<(&str, AppConfig, ManagerState<'_>)> = Vec::new();

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(true);
    cases.push(("current dir list focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Workspace));
    cases.push(("current dir mounts focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    cases.push(("current dir global mounts focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    cases.push(("current dir global mounts focus", config, state));

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 1;
    state.set_list_names_focused(true);
    cases.push(("saved workspace list focus", config, state));

    for (name, focus) in [
        ("saved workspace mounts focus", MountScrollFocus::Workspace),
        (
            "saved workspace global mounts focus",
            MountScrollFocus::Global,
        ),
        (
            "saved workspace role global mounts focus",
            MountScrollFocus::RoleGlobal,
        ),
        ("saved workspace roles focus", MountScrollFocus::Roles),
    ] {
        let config = detail_config();
        let mut state = ManagerState::from_config(&config, &cwd);
        state.selected = 1;
        state.set_list_names_focused(false);
        state.set_list_scroll_focus(Some(focus));
        cases.push((name, config, state));
    }

    let config = detail_config();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 2;
    state.set_list_names_focused(true);
    cases.push(("new workspace detail focus", config, state));

    for (name, config, mut state) in cases {
        let buf = render_manager_buffer(&mut state, &config, &cwd, 110, 30);
        assert!(
            focused_region_count(&buf) >= 1,
            "{name} must project focused state into the rendered composition"
        );
    }
}

#[test]
fn host_console_modal_states_project_visible_focus() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let cases = modal_focus_cases(&config, &cwd);

    for (name, mut state) in cases {
        let buf = render_manager_buffer(&mut state, &config, &cwd, 100, 28);
        assert!(
            focused_region_count(&buf) >= 1,
            "{name} must project focused state into the rendered composition"
        );
    }
}

#[test]
fn snapshot_global_mounts_110x30() {
    let config = detail_config();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.selected = 0;
    state.set_list_names_focused(false);
    state.set_list_scroll_focus(Some(MountScrollFocus::Global));
    let rendered = render_manager_state(&mut state, &config, &cwd, 110, 30);
    insta::assert_snapshot!("global_mounts_110x30", rendered);
}

#[test]
fn snapshot_editor_auth_tab_90x20() {
    let config = AppConfig::default();
    let cwd = test_cwd();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Auth;
    editor.set_tab_bar_focused(false);
    editor.set_tab_content_scroll_focused(true);
    state.stage = ManagerStage::Editor(editor);
    let rendered = render_manager_state(&mut state, &config, &cwd, 90, 20);
    insta::assert_snapshot!("editor_auth_tab_90x20", rendered);
}
