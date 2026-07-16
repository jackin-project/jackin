// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{AppConfig, WorkspaceConfig};
use ratatui::layout::Rect;
use std::path::PathBuf;
use termrock::{keymap::glyph, widgets::HintSpan};

use super::workspace_screen_footer_items_for_state;
use crate::tui::components::file_browser::FileBrowserState;
use crate::tui::components::footer_hints::editor_footer_items;
use crate::tui::model::ConsoleManagerStage;
use crate::tui::screens::settings::view::settings_screen_footer_for_state;
use crate::tui::state::{
    CreatePreludeState, FileBrowserTarget, ManagerState, Modal, SettingsModal, SettingsState,
    SettingsTab,
};

fn file_browser_state_at(path: PathBuf) -> FileBrowserState {
    FileBrowserState::from_listing(crate::services::file_browser::listing_at(
        path.clone(),
        path,
    ))
}

fn file_browser_state() -> FileBrowserState {
    let dir = tempfile::tempdir().unwrap();
    file_browser_state_at(dir.keep())
}

fn labels(items: Vec<HintSpan<'static>>) -> Vec<String> {
    items
        .into_iter()
        .filter_map(|span| match span {
            HintSpan::Key(value) | HintSpan::Text(value) => Some(value.to_owned()),
            HintSpan::Dyn(value) | HintSpan::DynKey(value) => Some(value),
            HintSpan::Sep | HintSpan::GroupSep => None,
        })
        .collect()
}

fn assert_file_browser_hints(items: Vec<HintSpan<'static>>) {
    let labels = labels(items);
    for expected in [
        "\u{2191}\u{2193}/j/k",
        "navigate",
        glyph::PGUP_PGDN,
        "page",
        "S",
        "select",
    ] {
        assert!(
            labels.iter().any(|label| label == expected),
            "missing {expected:?} in {labels:?}"
        );
    }
}

#[test]
fn list_file_browser_hints_reach_reserved_footer() {
    let config = AppConfig::default();
    let cwd = std::env::current_dir().unwrap();
    let mut state = ManagerState::from_config(&config, &cwd);
    state.list_modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: file_browser_state(),
    });

    assert_file_browser_hints(workspace_screen_footer_items_for_state(
        &state,
        &config,
        &cwd,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn create_prelude_file_browser_hints_reach_reserved_footer() {
    let config = AppConfig::default();
    let cwd = std::env::current_dir().unwrap();
    let mut state = ManagerState::from_config(&config, &cwd);
    let mut prelude = CreatePreludeState::new();
    prelude.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::CreateFirstMountSrc,
        state: file_browser_state(),
    });
    state.stage = ConsoleManagerStage::CreatePrelude(prelude);

    assert_file_browser_hints(workspace_screen_footer_items_for_state(
        &state,
        &config,
        &cwd,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn editor_file_browser_hints_reach_footer() {
    let config = AppConfig::default();
    let mut editor = crate::tui::state::EditorState::new_edit(
        "workspace".to_owned(),
        WorkspaceConfig::default(),
    );
    editor.modal = Some(Modal::FileBrowser {
        target: FileBrowserTarget::EditAddMountSrc,
        state: file_browser_state(),
    });

    assert_file_browser_hints(editor_footer_items(
        &editor,
        &config,
        false,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn settings_mounts_file_browser_hints_reach_footer() {
    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Mounts;
    settings.mounts.modal = Some(SettingsModal::MountFileBrowser {
        state: Box::new(file_browser_state()),
    });

    assert_file_browser_hints(settings_screen_footer_for_state(
        &settings,
        false,
        Rect::new(0, 0, 120, 40),
    ));
}

#[test]
fn settings_auth_file_browser_hints_reach_footer() {
    let config = AppConfig::default();
    let mut settings = SettingsState::from_config(&config);
    settings.active_tab = SettingsTab::Auth;
    settings.auth.modal = Some(SettingsModal::AuthSourceFolderPicker {
        state: file_browser_state(),
    });

    assert_file_browser_hints(settings_screen_footer_for_state(
        &settings,
        false,
        Rect::new(0, 0, 120, 40),
    ));
}
