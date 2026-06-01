//! Key dispatch for the workspace manager. Modal-first precedence:
//! if a modal is open, events go to the modal handler; otherwise they
//! go to the active stage's handler.

pub mod auth;
pub(crate) mod editor;
pub(crate) mod global_mounts;
pub(crate) mod list;
pub(crate) mod mouse;
pub(crate) mod prelude;
pub mod save;

use crossterm::event::KeyEvent;

use super::message::{ManagerEffect, ManagerMessage, execute_manager_effect, update_manager};
use super::state::{EditorSaveFlow, ExitIntent, ManagerStage, ManagerState};
use crate::config::AppConfig;
use crate::paths::JackinPaths;
use jackin_console::tui::effect::ConsoleEffect;
use jackin_tui::ModalOutcome;

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

#[allow(clippy::too_many_lines)]
pub fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> anyhow::Result<InputOutcome> {
    // List-level modal precedence (e.g. GithubPicker opened from `o` on a
    // workspace row, or AgentPicker opened from Enter when the highlighted
    // workspace has multiple eligible roles). Handled before stage-specific
    // modals so the dispatch stays uniform whatever stage the state thinks
    // it's in. Returns the modal's outcome directly — most arms produce
    // `Continue`, but `AgentPicker` commit produces `LaunchWithAgent`.
    if state.list_modal.is_some() {
        return Ok(list::handle_list_modal(state, key));
    }
    if state.inline_new_session_picker.is_some() {
        return Ok(list::handle_new_session_picker(state, key));
    }
    if state.inline_provider_picker.is_some() {
        return Ok(list::handle_inline_provider_picker(state, key));
    }
    if state.launch_provider_picker.is_some() {
        return Ok(list::handle_launch_provider_picker(state, key));
    }
    if state.inline_agent_picker.is_some() {
        return Ok(list::handle_inline_agent_picker(state, key));
    }
    if state.inline_role_picker.is_some() {
        return Ok(list::handle_inline_role_picker(state, key));
    }
    // Modal precedence: if a modal is open, it gets the event.
    // Use a discriminant check so we can take &mut without keeping an
    // immutable borrow alive across the call.
    // Capture `op_available` and the session-scoped op_cache from
    // the manager state before the editor borrow so the EnvKey commit
    // path can build a SourcePicker (knows if 1Password is selectable)
    // and the SourcePicker → OpPicker transition can construct a
    // cache-sharing picker.
    let op_available = state.op_available;
    let op_cache = state.op_cache.clone();
    if let ManagerStage::Editor(editor) = &mut state.stage
        && editor.modal.is_some()
    {
        let mut open_url = None;
        let editor_outcome = editor::handle_editor_modal(
            editor,
            key,
            op_available,
            op_cache,
            config,
            paths,
            &mut open_url,
        );
        match editor_outcome {
            editor::EditorModalOutcome::Continue => {}
            editor::EditorModalOutcome::StartRoleRegistration {
                raw,
                key,
                selector,
                source,
            } => {
                execute_manager_effect(
                    state,
                    config,
                    paths,
                    ManagerEffect::StartRoleRegistration {
                        raw,
                        key,
                        selector,
                        source,
                    },
                );
            }
            editor::EditorModalOutcome::ValidateOpRef(op_ref) => {
                execute_manager_effect(
                    state,
                    config,
                    paths,
                    ManagerEffect::ValidateOpCommit {
                        op_ref,
                        is_settings: false,
                    },
                );
            }
        }
        execute_manager_effect(
            state,
            config,
            paths,
            ConsoleEffect::RequestActiveMountInfoRefresh.into(),
        );
        if let Some(url) = open_url {
            return Ok(InputOutcome::OpenUrl(url));
        }

        // Drain the ConfirmSave → commit signal FIRST. The modal handler
        // only closes the modal and stashes the plan; this outer layer
        // has `paths`/`cwd` and actually performs the write.
        let pending = if let ManagerStage::Editor(editor) = &mut state.stage {
            match std::mem::replace(&mut editor.save_flow, EditorSaveFlow::Idle) {
                EditorSaveFlow::PendingCommit {
                    plan,
                    exit_on_success,
                } => Some((plan, exit_on_success)),
                other => {
                    // Not a commit transition — put the flow back untouched.
                    editor.save_flow = other;
                    None
                }
            }
        } else {
            None
        };
        if let Some((plan, exit_on_success)) = pending {
            save::commit_editor_save(state, config, paths, cwd, plan, exit_on_success)?;
            return Ok(InputOutcome::Continue);
        }

        // After modal handling, check if an exit intent was signalled by
        // the SaveDiscardCancel modal.
        let intent = if let ManagerStage::Editor(editor) = &state.stage {
            editor.exit_after_save
        } else {
            None
        };
        if let Some(intent) = intent {
            match intent {
                ExitIntent::Save => {
                    // Route through the two-phase save: ConfirmSave opens
                    // first; the eventual commit is the one that exits.
                    // Pass `exit_on_success = true` so that, if the operator
                    // picks Save in the confirm dialog and the write
                    // succeeds, we bounce out to the workspace list.
                    if let ManagerStage::Editor(e) = &mut state.stage {
                        e.exit_after_save = None;
                    }
                    save::begin_editor_save(state, config, true)?;
                }
                ExitIntent::Discard => {
                    let _ = update_manager(
                        state,
                        ManagerMessage::ReloadFromConfig {
                            config: Box::new(config.clone()),
                            cwd: cwd.to_path_buf(),
                        },
                    );
                }
            }
            return Ok(InputOutcome::Continue);
        }
        return Ok(InputOutcome::Continue);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && settings.error_popup.is_some()
    {
        let dismiss = settings.error_popup.as_ref().is_some_and(|p| {
            matches!(
                p.handle_key(key),
                jackin_tui::ModalOutcome::Cancel
            )
        });
        if dismiss {
            let _ = update_manager(state, ManagerMessage::DismissSettingsErrorPopup);
        }
        return Ok(InputOutcome::Continue);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && settings.mounts.modal.is_some()
    {
        let mut open_url = None;
        let modal_outcome =
            global_mounts::handle_settings_confirm_modal(settings, key, &mut open_url);
        if matches!(
            modal_outcome,
            global_mounts::SettingsModalOutcome::SaveSettings
        ) {
            execute_manager_effect(state, config, paths, ConsoleEffect::SaveSettings.into());
        }
        global_mounts::after_settings_event(state);
        if let Some(url) = open_url {
            return Ok(InputOutcome::OpenUrl(url));
        }
        return Ok(InputOutcome::Continue);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && settings.env.modal.is_some()
    {
        global_mounts::handle_settings_env_modal(&mut settings.env, key, op_cache);
        global_mounts::after_settings_event(state);
        return Ok(InputOutcome::Continue);
    }
    if let ManagerStage::Settings(settings) = &mut state.stage
        && settings.auth.modal.is_some()
    {
        let auth_outcome = global_mounts::handle_settings_auth_modal(
            &mut settings.auth,
            &mut settings.env,
            &mut settings.pending_token_generate,
            key,
            op_available,
            op_cache,
        );
        if let global_mounts::SettingsAuthOutcome::ValidateOpRef(op_ref) = auth_outcome {
            execute_manager_effect(
                state,
                config,
                paths,
                ManagerEffect::ValidateOpCommit {
                    op_ref,
                    is_settings: true,
                },
            );
        }
        global_mounts::after_settings_event(state);
        return Ok(InputOutcome::Continue);
    }
    if matches!(state.stage, ManagerStage::CreatePrelude(_)) {
        let has_modal = if let ManagerStage::CreatePrelude(p) = &state.stage {
            p.modal.is_some()
        } else {
            false
        };
        if has_modal {
            let outcome = if let ManagerStage::CreatePrelude(p) = &mut state.stage {
                prelude::handle_prelude_modal(p, key)
            } else {
                InputOutcome::Continue
            };
            if !matches!(outcome, InputOutcome::Continue) {
                return Ok(outcome);
            }
            // After the modal handler runs, the prelude is in one of three states:
            // - still in a modal (user pressed a non-commit/cancel key): continue
            // - modal cleared + completed() Some: wizard done → transition to Editor
            // - modal cleared + completed() None: wizard cancelled → back to List
            //
            // `completed()` checks every required field together and
            // returns the owned (name, ws) pair so we don't need a
            // separate `pending_name.is_some()` flag plus an
            // `expect("prelude complete")` to keep the two invariants
            // in sync.
            // `WorkspaceConfig` is several hundred bytes once auth /
            // canonical-slot fields are populated, so box the
            // success carrier — `Complete` was already the only
            // payload variant.
            #[allow(clippy::items_after_statements)]
            enum PreludeStatus {
                InProgress,
                Complete(Box<(String, crate::workspace::WorkspaceConfig)>),
                Cancelled,
            }
            let status = if let ManagerStage::CreatePrelude(p) = &state.stage {
                if p.modal.is_some() {
                    PreludeStatus::InProgress
                } else if let Some((name, ws)) = p.completed() {
                    PreludeStatus::Complete(Box::new((name, ws)))
                } else {
                    PreludeStatus::Cancelled
                }
            } else {
                PreludeStatus::InProgress
            };
            match status {
                PreludeStatus::Complete(payload) => {
                    let (name, ws) = *payload;
                    let _ = update_manager(
                        state,
                        ManagerMessage::EnterCreateEditor {
                            name,
                            workspace: ws,
                        },
                    );
                }
                PreludeStatus::Cancelled => {
                    let _ = update_manager(
                        state,
                        ManagerMessage::ReloadFromConfig {
                            config: Box::new(config.clone()),
                            cwd: cwd.to_path_buf(),
                        },
                    );
                }
                PreludeStatus::InProgress => {}
            }
            return Ok(InputOutcome::Continue);
        }
    }

    // Non-modal routing per stage — capture which stage we're in as a
    // simple enum discriminant so the immutable borrow ends before we
    // pass &mut state into the stage handler.
    #[allow(clippy::items_after_statements)]
    enum StageDis {
        List,
        Editor,
        Settings,
        CreatePrelude,
        ConfirmDelete,
        ConfirmInstancePurge,
    }
    let dis = match &state.stage {
        ManagerStage::List => StageDis::List,
        ManagerStage::Editor(_) => StageDis::Editor,
        ManagerStage::Settings(_) => StageDis::Settings,
        ManagerStage::CreatePrelude(_) => StageDis::CreatePrelude,
        ManagerStage::ConfirmDelete { .. } => StageDis::ConfirmDelete,
        ManagerStage::ConfirmInstancePurge { .. } => StageDis::ConfirmInstancePurge,
    };

    let outcome = match dis {
        StageDis::List => list::handle_list_key(state, config, paths, cwd, key),
        StageDis::Editor => editor::handle_editor_key(state, config, paths, cwd, key),
        StageDis::Settings => {
            let mut open_url = None;
            global_mounts::handle_settings_key_with_open_url(state, key, &mut open_url);
            global_mounts::after_settings_event(state);
            Ok(open_url.map_or(InputOutcome::Continue, InputOutcome::OpenUrl))
        }
        StageDis::CreatePrelude => Ok(prelude::handle_prelude_key(state, config, paths, cwd, key)),
        StageDis::ConfirmDelete => Ok(handle_confirm_delete_key(state, key)),
        StageDis::ConfirmInstancePurge => Ok(handle_confirm_instance_purge_key(state, key)),
    };
    execute_manager_effect(
        state,
        config,
        paths,
        ConsoleEffect::RequestActiveMountInfoRefresh.into(),
    );
    outcome
}

fn handle_confirm_instance_purge_key(state: &mut ManagerState<'_>, key: KeyEvent) -> InputOutcome {
    let ManagerStage::ConfirmInstancePurge {
        container,
        state: confirm_state,
        ..
    } = &mut state.stage
    else {
        return InputOutcome::Continue;
    };
    let outcome = confirm_state.handle_key(key);
    let container_name = container.clone();
    match outcome {
        ModalOutcome::Commit(true) => {
            let _ = update_manager(state, ManagerMessage::ReturnToList);
            InputOutcome::InstanceAction {
                container: container_name,
                action: crate::console::ConsoleInstanceAction::Purge,
            }
        }
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
            let _ = update_manager(state, ManagerMessage::ReturnToList);
            InputOutcome::Continue
        }
        ModalOutcome::Continue => InputOutcome::Continue,
    }
}

fn handle_confirm_delete_key(
    state: &mut ManagerState<'_>,
    key: KeyEvent,
) -> InputOutcome {
    let ManagerStage::ConfirmDelete {
        name,
        state: confirm_state,
    } = &mut state.stage
    else {
        return InputOutcome::Continue;
    };
    let outcome = confirm_state.handle_key(key);
    let ws_name = name.clone();
    match outcome {
        ModalOutcome::Commit(true) => {
            let _ = update_manager(state, ManagerMessage::ReturnToList);
            InputOutcome::RemoveWorkspace(ws_name)
        }
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => {
            let _ = update_manager(state, ManagerMessage::ReturnToList);
            InputOutcome::Continue
        }
        ModalOutcome::Continue => InputOutcome::Continue,
    }
}

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
