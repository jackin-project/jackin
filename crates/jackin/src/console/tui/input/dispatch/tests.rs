//! Cross-flow tests that genuinely span multiple stages. Stage-local
//! tests live in the matching `input/<stage>.rs` test module:
//! `input/list.rs`, `input/editor.rs`, `input/save.rs`,
//! `input/prelude.rs`, `input/mouse.rs`.
//!
//! Anything kept here must drive a transition that crosses two stage
//! handlers in a single test (e.g. open the in-editor rename modal,
//! commit it via `handle_key`, then drive the save flow through the
//! same `handle_key`).
use super::super::test_support::{key, mount};
use super::*;
use crate::console::tui::state::{
    EditorState, FieldFocus, ManagerStage, ManagerState, SettingsState,
};
use crossterm::event::KeyCode;
use jackin_config::AppConfig;
use jackin_core::JackinPaths;

/// End-to-end: start Create, rename via Enter-on-row-0, commit the
/// save, and verify the workspace on disk has the updated name.
/// Spans editor (rename modal) and save (commit) — a true cross-flow
/// test that doesn't fit cleanly inside either submodule.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "test waits for an owned background save worker to publish its subscription result"
)]
fn create_mode_save_uses_updated_pending_name() {
    let (tmp, paths, mut config) = {
        let tmp = tempfile::tempdir().unwrap();
        let paths = JackinPaths::for_tests(tmp.path());
        paths.ensure_base_dirs().unwrap();
        let config = AppConfig::default();
        let toml = toml::to_string(&config).unwrap();
        std::fs::write(&paths.config_file, toml).unwrap();
        let loaded = AppConfig::load_or_init(&paths).unwrap();
        (tmp, paths, loaded)
    };
    let cwd = tmp.path();
    let mut state = ManagerState::from_config(&config, cwd);
    let mut editor = EditorState::new_create();
    editor.pending_name = Some("original".into());
    editor.pending.workdir = "/code/proj".into();
    editor.pending.mounts = vec![mount("/code/proj", "/code/proj")];
    editor.active_field = FieldFocus::Row(0);
    state.stage = ManagerStage::Editor(editor);

    // Open rename modal via Enter on row 0.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    // Clear the pre-filled "original" and type "renamed".
    for _ in 0..8 {
        handle_key(
            &mut state,
            &mut config,
            &paths,
            cwd,
            key(KeyCode::Backspace),
        )
        .unwrap();
    }
    for ch in "renamed".chars() {
        handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Char(ch))).unwrap();
    }
    // Commit the TextInput.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();

    // Kick off the save: `s` → ConfirmSave → Enter commits.
    handle_key(
        &mut state,
        &mut config,
        &paths,
        cwd,
        key(KeyCode::Char('s')),
    )
    .unwrap();
    // Default focus = Cancel; Tab -> Save, then Enter commits.
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Tab)).unwrap();
    handle_key(&mut state, &mut config, &paths, cwd, key(KeyCode::Enter)).unwrap();
    crate::console::effects::execute_pending_workspace_save_commit(
        &mut state,
        &mut config,
        &paths,
        cwd,
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while std::time::Instant::now() < deadline {
        if let Some(result) = state.poll_config_save() {
            crate::console::effects::apply_background_event(
                &mut state,
                &mut config,
                &paths,
                cwd,
                jackin_console::tui::state::update::ManagerBackgroundEvent::ConfigSaveFinished(
                    result,
                ),
            );
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let reloaded = AppConfig::load_or_init(&paths).unwrap();
    assert!(
        reloaded.workspaces.contains_key("renamed"),
        "save must persist the edited name; got workspaces={:?}",
        reloaded.workspaces.keys().collect::<Vec<_>>()
    );
    assert!(
        !reloaded.workspaces.contains_key("original"),
        "the original (pre-edit) name must not end up on disk"
    );
}

#[test]
fn settings_error_popup_dismissed_by_enter() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.error_popup = Some(jackin_tui::components::ErrorPopupState::new(
        "Test", "details",
    ));
    state.stage = ManagerStage::Settings(settings);

    let outcome = handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Enter),
    )
    .unwrap();

    assert!(
        matches!(outcome, InputOutcome::Continue),
        "Enter on error popup must return Continue; got {outcome:?}"
    );
    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("must remain in Settings stage");
    };
    assert!(
        settings.error_popup.is_none(),
        "Enter must dismiss the error popup"
    );
}

#[test]
fn settings_error_popup_unrelated_key_does_not_dismiss() {
    let tmp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(tmp.path());
    paths.ensure_base_dirs().unwrap();
    let mut config = AppConfig::default();
    let mut state = ManagerState::from_config(&config, tmp.path());
    let mut settings = SettingsState::from_config(&config);
    settings.error_popup = Some(jackin_tui::components::ErrorPopupState::new(
        "Test", "details",
    ));
    state.stage = ManagerStage::Settings(settings);

    handle_key(
        &mut state,
        &mut config,
        &paths,
        tmp.path(),
        key(KeyCode::Char('j')),
    )
    .unwrap();

    let ManagerStage::Settings(settings) = &state.stage else {
        panic!("must remain in Settings stage");
    };
    assert!(
        settings.error_popup.is_some(),
        "unrelated key must not dismiss the error popup"
    );
}
