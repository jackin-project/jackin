// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn handle_modal(editor: &mut EditorState<'_>, k: crossterm::event::KeyEvent) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    handle_modal_with(editor, k, &mut config, &paths);
}

pub(super) fn handle_modal_with(
    editor: &mut EditorState<'_>,
    k: crossterm::event::KeyEvent,
    config: &mut AppConfig,
    paths: &JackinPaths,
) {
    let outcome = handle_editor_modal(
        editor,
        k,
        false,
        std::rc::Rc::new(std::cell::RefCell::new(OpCache::default())),
        config,
        paths,
        Rect::new(0, 0, 120, 40),
    );
    match outcome {
        EditorModalOutcome::PersistTrustedRoleSource { key, mut source } => {
            source.trusted = true;
            crate::console::effects::persist_trusted_role_source_for_tests(
                editor, config, paths, &key, &source,
            );
        }
        EditorModalOutcome::OpenUrl(_) => panic!("test helper did not expect URL-open"),
        _ => {}
    }
}

pub(super) fn apply_text_input(
    target: &TextInputTarget,
    editor: &mut EditorState<'_>,
    value: &str,
) {
    apply_text_input_to_pending(target, editor, value, false);
}

pub(super) fn empty_ws() -> WorkspaceConfig {
    WorkspaceConfig::default()
}

pub(super) fn config_with_agents(names: &[&str]) -> AppConfig {
    use jackin_config::fixtures::config_with_agents as make_config;
    let mut config = make_config(names);
    for name in names {
        if let Some(role) = config.roles.get_mut(*name) {
            role.git = format!("https://example.test/{name}.git");
        }
    }
    config.workspaces.insert("ws".into(), empty_ws());
    config
}

pub(super) fn seed_first_temp_valid_role_repo(data_dir: &std::path::Path) {
    seed_valid_role_repo(&first_temp_role_repo(data_dir));
}

pub(super) fn editor_on_agents_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Roles;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(row);
    state.stage = ManagerStage::Editor(editor);
    state
}

pub(super) fn editor_on_mounts_tab<'a>(ws: WorkspaceConfig, row: usize) -> ManagerState<'a> {
    let mut state = ManagerState::from_config(&AppConfig::default(), std::path::Path::new("/"));
    let mut editor = EditorState::new_edit("ws".into(), ws);
    editor.active_tab = EditorTab::Mounts;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(row);
    state.stage = ManagerStage::Editor(editor);
    state
}

pub(super) fn ws_with_one_mount(readonly: bool) -> WorkspaceConfig {
    WorkspaceConfig {
        mounts: vec![MountConfig {
            src: "/host/a".into(),
            dst: "/host/a".into(),
            readonly,
            isolation: jackin_config::MountIsolation::Shared,
        }],
        ..WorkspaceConfig::default()
    }
}

pub(super) fn press(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    code: KeyCode,
) -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs()?;
    handle_key(state, config, &paths, tmp.path(), key(code))?;
    Ok(())
}

pub(super) fn pending_allowed(state: &ManagerState<'_>) -> Vec<String> {
    let ManagerStage::Editor(e) = &state.stage else {
        panic!("editor stage expected");
    };
    e.pending.allowed_roles.clone()
}

pub(super) fn editor_with_browser_committed(src: &str) -> EditorState<'static> {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    apply_file_browser_to_editor(
        FileBrowserTarget::EditAddMountSrc,
        &mut editor,
        std::path::PathBuf::from(src),
    );
    editor
}

pub(super) fn editor_with_file_browser_parent_committed(src: &str) -> EditorState<'static> {
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = EditorTab::Mounts;
    editor.set_tab_bar_focused(false);
    editor.active_field = FieldFocus::Row(0);
    editor.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: jackin_console::tui::components::file_browser::FileBrowserState::from_listing(
            jackin_console::services::file_browser::listing_from_home().unwrap(),
        ),
    });
    apply_file_browser_to_editor(
        FileBrowserTarget::EditAddMountSrc,
        &mut editor,
        std::path::PathBuf::from(src),
    );
    editor
}

pub(super) fn editor_state_on_tab(
    start_tab: EditorTab,
) -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut editor = EditorState::new_edit("ws".into(), WorkspaceConfig::default());
    editor.active_tab = start_tab;
    editor.set_tab_bar_focused(false);
    state.stage = ManagerStage::Editor(editor);
    (state, config, paths, tmp)
}
