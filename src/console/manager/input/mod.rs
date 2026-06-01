//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

pub mod auth;
mod dispatch;
pub(crate) mod editor;
pub(crate) mod global_mounts;
pub(crate) mod list;
pub(crate) mod mouse;
pub(crate) mod prelude;
pub mod save;

pub use dispatch::handle_key;
pub use mouse::{clickable_at, handle_mouse, handle_mouse_with_config};

// Re-exported for the `run_console` token-generate loop, which re-mounts
// the settings auth form after a mint (the `global_mounts` module is
// `pub(super)`, so the loop reaches the helpers through this seam).
pub(in crate::console) use global_mounts::{
    apply_op_picker_settings_commit_failed, apply_op_picker_to_settings_auth_form_committed,
    apply_plain_text_to_settings_auth_form,
};

pub type InputOutcome = jackin_console::tui::message::ConsoleInputOutcome<
    crate::selector::RoleSelector,
    crate::agent::Agent,
    crate::console::ConsoleInstanceAction,
    jackin_protocol::Provider,
>;

pub(super) use super::file_browser::{
    apply_outcome as apply_file_browser_outcome, clamp_to_cwd as clamp_file_browser_to_cwd,
    from_home as new_file_browser_from_home,
    request_git_url_resolution as request_file_browser_git_url_resolution,
};

/// Cross-submodule helpers for the input/* test modules. Lifted out of
/// the per-submodule test blocks because `key()` and `mount()` show up in
/// virtually every test file; keeping a single canonical definition
/// avoids the previous problem where each submodule grew its own
/// near-identical copy.
#[cfg(test)]
pub(super) mod test_support {
    use crate::workspace::MountConfig;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    pub fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    pub fn mount(src: &str, dst: &str) -> MountConfig {
        MountConfig {
            src: src.into(),
            dst: dst.into(),
            readonly: false,
            isolation: crate::isolation::MountIsolation::Shared,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Cross-flow tests that genuinely span multiple stages. Stage-local
    //! tests live in the matching `input/<stage>.rs` test module:
    //! `input/list.rs`, `input/editor.rs`, `input/save.rs`,
    //! `input/prelude.rs`, `input/mouse.rs`.
    //!
    //! Anything kept here must drive a transition that crosses two stage
    //! handlers in a single test (e.g. open the in-editor rename modal,
    //! commit it via `handle_key`, then drive the save flow through the
    //! same `handle_key`).
    use super::super::state::{EditorState, FieldFocus, ManagerStage, ManagerState};
    use super::test_support::{key, mount};
    use super::*;
    use crate::config::AppConfig;
    use crate::paths::JackinPaths;
    use crossterm::event::KeyCode;

    /// End-to-end: start Create, rename via Enter-on-row-0, commit the
    /// save, and verify the workspace on disk has the updated name.
    /// Spans editor (rename modal) and save (commit) — a true cross-flow
    /// test that doesn't fit cleanly inside either submodule.
    #[test]
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
        let mut settings = super::super::state::SettingsState::from_config(&config);
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
        let mut settings = super::super::state::SettingsState::from_config(&config);
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
}
