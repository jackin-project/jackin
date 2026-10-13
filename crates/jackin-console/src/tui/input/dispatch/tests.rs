// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `dispatch`: keyboard-help overlay routing (`?` opens from every
//! stage, the open overlay owns keys, Esc returns to the stage).

use super::*;
use crate::tui::components::ConfirmState;
use crate::tui::state::{EditorState, SettingsState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::TempDir;

fn harness() -> (ManagerState<'static>, AppConfig, JackinPaths, TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let config = AppConfig::default();
    let state = ManagerState::from_config(&config, tmp.path());
    (state, config, paths, tmp)
}

fn press_question_mark(state: &mut ManagerState<'_>, config: &mut AppConfig, paths: &JackinPaths) {
    let cwd = std::path::PathBuf::from("/tmp/jackin-test");
    handle_key(
        state,
        config,
        paths,
        &cwd,
        KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .unwrap();
}

#[test]
fn question_mark_opens_keyboard_help_from_every_stage() {
    let (mut state, mut config, paths, _tmp) = harness();

    let stages: Vec<ManagerStage<'_>> = vec![
        ManagerStage::List,
        ManagerStage::Editor(EditorState::new_create()),
        ManagerStage::Settings(SettingsState::from_config(&config)),
        ManagerStage::CreatePrelude(crate::tui::state::CreatePreludeState::default()),
        ManagerStage::ConfirmDelete {
            name: "alpha".to_owned(),
            state: ConfirmState::new("Delete workspace?"),
        },
        ManagerStage::ConfirmInstancePurge {
            container: "abc".to_owned(),
            label: "ws".to_owned(),
            state: ConfirmState::new("Purge instance?"),
        },
    ];
    for stage in stages {
        state.stage = stage;
        state.keyboard_help = None;
        press_question_mark(&mut state, &mut config, &paths);
        assert!(
            state.keyboard_help.is_some(),
            "stage {:?} must open the help overlay",
            state.stage.route()
        );
    }
}

#[test]
fn esc_dismisses_keyboard_help_and_returns_keys_to_stage() {
    let (mut state, mut config, paths, _tmp) = harness();
    let cwd = std::path::PathBuf::from("/tmp/jackin-test");

    press_question_mark(&mut state, &mut config, &paths);
    assert!(state.keyboard_help.is_some());

    handle_key(
        &mut state,
        &mut config,
        &paths,
        &cwd,
        KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        &|_, _| Ok(()),
    )
    .unwrap();
    assert!(state.keyboard_help.is_none());
}

#[test]
fn open_modal_owns_question_mark_as_typed_input() {
    let (mut state, mut config, paths, _tmp) = harness();
    state.open_list_error_popup("Docker daemon not reachable", "docker socket missing");

    press_question_mark(&mut state, &mut config, &paths);
    assert!(
        state.keyboard_help.is_none(),
        "list modal must own `?` — the overlay must not open over it"
    );
    assert!(state.list_modal.is_some());
}
