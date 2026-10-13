// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

