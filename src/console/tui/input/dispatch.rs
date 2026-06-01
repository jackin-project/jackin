//! Key dispatch for the workspace manager.

use crossterm::event::KeyEvent;

use super::super::effect::ManagerEffect;
use crate::console::tui::message::{ManagerMessage, update_manager};
use crate::console::tui::state::{ExitIntent, ManagerStage, ManagerState};
use super::{InputOutcome, editor, global_mounts, list, prelude, save};
use crate::config::AppConfig;
use crate::paths::JackinPaths;
use jackin_console::tui::effect::ConsoleEffect;
use jackin_tui::ModalOutcome;

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
                state.request_effect(ManagerEffect::StartRoleRegistration {
                    raw,
                    key,
                    selector,
                    source,
                });
            }
            editor::EditorModalOutcome::PersistTrustedRoleSource { key, source } => {
                state.request_effect(ManagerEffect::PersistTrustedRoleSource { key, source });
            }
            editor::EditorModalOutcome::ValidateOpRef(op_ref) => {
                state.request_effect(ManagerEffect::ValidateOpCommit {
                    op_ref,
                    is_settings: false,
                });
            }
        }
        state.request_effect(ConsoleEffect::RequestActiveMountInfoRefresh.into());
        if let Some(url) = open_url {
            return Ok(InputOutcome::OpenUrl(url));
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
            state.request_effect(ConsoleEffect::SaveSettings.into());
        }
        if matches!(
            modal_outcome,
            global_mounts::SettingsModalOutcome::OpenGlobalMountFileBrowser
        ) {
            state.request_effect(ManagerEffect::OpenGlobalMountFileBrowser);
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
            state.request_effect(ManagerEffect::ValidateOpCommit {
                op_ref,
                is_settings: true,
            });
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
                if matches!(outcome, InputOutcome::OpenCreatePreludeFileBrowserAtLastCwd) {
                    state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowserAtLastCwd);
                    return Ok(InputOutcome::Continue);
                }
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
    }?;
    let outcome = match outcome {
        InputOutcome::OpenCreatePreludeFileBrowserAtLastCwd => {
            state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowserAtLastCwd);
            InputOutcome::Continue
        }
        other => other,
    };
    state.request_effect(ConsoleEffect::RequestActiveMountInfoRefresh.into());
    Ok(outcome)
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
