// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn editor_non_mounts_tab_click_focuses_horizontal_scroll_block() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_width = 80;
    editor.tab_content_height = 4;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(&mut state, mouse_at(10, 6), term(42), None);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(editor.tab_content_scroll_focused());

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollRight, 10, 6),
        term(42),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll.offset_x(), MOUSE_HORIZONTAL_SCROLL_STEP);
    assert!(editor.tab_content_scroll_focused());
}

#[test]
fn editor_vertical_wheel_scrolls_only_inside_content_area() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 1),
        term(100),
        None,
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll.offset_y(), 0);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 6),
        term(100),
        None,
    );
    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll.offset_y(), 1);
}

#[test]
fn editor_general_tab_vertical_wheel_uses_shared_scroll_path() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::General;
    editor.tab_content_height = 4;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 6),
        Rect::new(0, 0, 100, 9),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll.offset_y(),
        1,
        "General must use the same vertical wheel path as every editor tab"
    );
}

#[test]
fn editor_general_tab_vertical_scrollbar_drag_uses_shared_scroll_path() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::General;
    editor.tab_content_height = 4;
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Down(MouseButton::Left), 99, 7),
        Rect::new(0, 0, 100, 10),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert!(
        editor.tab_scroll.offset_y() > 0,
        "General scrollbar dragging must use the same vertical path as every editor tab"
    );
}

#[test]
fn editor_vertical_wheel_ignores_background_when_modal_open() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::SaveDiscardCancel {
        state: editor_exit_save_discard_state(),
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 10, 6),
        term(100),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll.offset_y(), 0);
}

#[test]
fn editor_file_browser_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::EditAddMountSrc,
        state: fb,
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll.offset_y(),
        0,
        "background editor must not scroll"
    );
    let Some(Modal::FileBrowser { state: fb, .. }) = &editor.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected().copied(), Some(1));
}

#[test]
fn editor_file_browser_smoke_hints_pagedown_and_wheel_share_modal_context() {
    let config = jackin_config::AppConfig::default();
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 10);
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::EditAddMountSrc,
        state: fb,
    });
    state.stage = ManagerStage::Editor(editor);

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    let hints = format!(
        "{:?}",
        crate::tui::components::footer_hints::editor_footer_items(
            editor,
            &config,
            false,
            Rect::new(0, 0, 120, 40),
        )
    );
    assert!(
        hints.contains(termrock::keymap::glyph::PGUP_PGDN),
        "footer hints missing page keys: {hints}"
    );

    let ManagerStage::Editor(editor) = &mut state.stage else {
        panic!("editor stage expected");
    };
    let Some(Modal::FileBrowser { state: fb, .. }) = &mut editor.modal else {
        panic!("file browser modal expected");
    };
    drop(fb.handle_key_with_page_rows(key(KeyCode::PageDown), Some(4)));
    assert_eq!(fb.list_state.selected().copied(), Some(4));

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        Some(&config),
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll.offset_y(),
        0,
        "background editor must not scroll"
    );
    let Some(Modal::FileBrowser { state: fb, .. }) = &editor.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected().copied(), Some(5));
}

#[test]
fn create_prelude_file_browser_wheel_scrolls_modal_selection() {
    use crate::tui::state::CreatePreludeState;

    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    state.stage = ManagerStage::CreatePrelude(CreatePreludeState {
        modal: Some(Modal::FileBrowser {
            target: crate::tui::state::FileBrowserTarget::CreateFirstMountSrc,
            state: fb,
        }),
        ..CreatePreludeState::default()
    });

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::CreatePrelude(prelude) = &state.stage else {
        panic!("create prelude stage expected");
    };
    let Some(Modal::FileBrowser { state: fb, .. }) = &prelude.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected().copied(), Some(1));
}

#[test]
fn settings_mounts_file_browser_wheel_scrolls_modal_selection_not_background() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    crate::tui::scroll_block::scroll_area_set_y(&mut settings.mounts.scroll, 4);
    settings
        .mounts
        .modals
        .open(SettingsModal::MountFileBrowser {
            state: Box::new(fb),
        });
    state.stage = ManagerStage::Settings(settings);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    assert_eq!(
        settings.mounts.scroll.offset_y(),
        4,
        "background settings must not scroll"
    );
    let Some(SettingsModal::MountFileBrowser { state: fb }) = settings.mounts.modals.current()
    else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected().copied(), Some(1));
}

#[test]
fn settings_auth_source_folder_wheel_scrolls_modal_selection() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut settings = SettingsState::from_config(&jackin_config::AppConfig::default());
    settings
        .auth
        .modals
        .open(SettingsModal::AuthSourceFolderPicker { state: fb });
    state.stage = ManagerStage::Settings(settings);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollDown, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("settings stage expected");
    };
    let Some(SettingsModal::AuthSourceFolderPicker { state: fb }) = settings.auth.modals.current()
    else {
        panic!("source-folder file browser modal expected");
    };
    assert_eq!(fb.list_state.selected().copied(), Some(1));
}

#[test]
fn file_browser_wheel_at_edge_is_consumed_before_background_scroll() {
    let mut state = list_state();
    let tmp = tempfile::tempdir().unwrap();
    let fb = file_browser_with_dirs(tmp.path(), 8);
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::FileBrowser {
        target: crate::tui::state::FileBrowserTarget::EditAddMountSrc,
        state: fb,
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::ScrollUp, 20, 11),
        term_120x40(),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(
        editor.tab_scroll.offset_y(),
        0,
        "saturated modal wheel must not leak"
    );
    let Some(Modal::FileBrowser { state: fb, .. }) = &editor.modal else {
        panic!("file browser modal expected");
    };
    assert_eq!(fb.list_state.selected().copied(), Some(0));
}

#[test]
fn editor_vertical_scrollbar_drag_ignores_background_when_modal_open() {
    let mut state = list_state();
    let mut editor = EditorState::new_edit("x".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Roles;
    editor.tab_content_height = 50;
    editor.modal = Some(Modal::SaveDiscardCancel {
        state: editor_exit_save_discard_state(),
    });
    state.stage = ManagerStage::Editor(editor);

    handle_mouse_with_config(
        &mut state,
        mouse_kind_at(MouseEventKind::Down(MouseButton::Left), 99, 7),
        term(100),
        None,
    );

    let ManagerStage::Editor(editor) = &state.stage else {
        panic!("editor stage expected");
    };
    assert_eq!(editor.tab_scroll.offset_y(), 0);
}
