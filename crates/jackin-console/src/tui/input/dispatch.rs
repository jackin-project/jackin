//! Key dispatch for the workspace manager.

use std::rc::Rc;

use crossterm::event::KeyEvent;
use jackin_config::AppConfig;
use jackin_core::JackinPaths;

use super::InputOutcome;
use super::editor::{EditorModalOutcome, handle_editor_key, handle_editor_modal};
use super::global_mounts::{
    SettingsAuthOutcome, SettingsModalOutcome, after_settings_event, handle_settings_auth_modal,
    handle_settings_confirm_modal, handle_settings_env_modal, handle_settings_key_with_effects,
};
use super::list::{
    handle_inline_agent_picker, handle_inline_provider_picker, handle_inline_role_picker,
    handle_launch_provider_picker, handle_list_key, handle_list_modal, handle_new_session_picker,
};
use super::prelude::{PreludeModalOutcome, handle_prelude_key, handle_prelude_modal};
use super::save::begin_editor_save;
use crate::tui::effect::{ConsoleEffect, FileBrowserEffectContext};
use crate::tui::model::{
    ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsoleManagerStageRoute,
    CreatePreludeCompletionStatus, console_input_dispatch_plan, create_prelude_completion_status,
};
use crate::tui::screens::workspaces::update::{
    InstancePurgeKeyPlan, WorkspaceDeleteKeyPlan, instance_purge_key_plan,
    workspace_delete_key_plan,
};
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{ExitIntent, ManagerEffect, ManagerStage, ManagerState};
use crate::tui::update::{DismissibleModalPlan, dismissible_modal_plan};

type ValidateAuthSourceFolder =
    dyn Fn(Option<crate::tui::auth::AuthKind>, &std::path::Path) -> Result<(), String>;

#[allow(
    clippy::too_many_lines,
    reason = "Top-level input dispatcher handling every per-stage key binding \
              inline. Each stage's arm carries its own focused state transition; \
              extracting arms into sub-dispatchers would require re-borrowing \
              state + config across fn boundaries."
)]
pub fn handle_key(
    state: &mut ManagerState<'_>,
    config: &mut AppConfig,
    paths: &JackinPaths,
    cwd: &std::path::Path,
    key: KeyEvent,
    validate_auth_source_folder: &ValidateAuthSourceFolder,
) -> anyhow::Result<InputOutcome> {
    let stage_modal_facts = state.stage.modal_facts();
    let dispatch_plan = console_input_dispatch_plan(ConsoleInputDispatchFacts {
        list_modal_open: state.list_modal.is_some(),
        inline_new_session_picker_open: state.inline_new_session_picker.is_some(),
        inline_provider_picker_open: state.inline_provider_picker.is_some(),
        launch_provider_picker_open: state.launch_provider_picker.is_some(),
        inline_agent_picker_open: state.inline_agent_picker.is_some(),
        inline_role_picker_open: state.inline_role_picker.is_some(),
        editor_modal_open: stage_modal_facts.editor_modal_open,
        settings_error_popup_open: stage_modal_facts.settings_error_popup_open,
        settings_mounts_modal_open: stage_modal_facts.settings_mounts_modal_open,
        settings_env_modal_open: stage_modal_facts.settings_env_modal_open,
        settings_auth_modal_open: stage_modal_facts.settings_auth_modal_open,
        create_prelude_modal_open: stage_modal_facts.create_prelude_modal_open,
        stage_route: state.stage.route(),
    });

    match dispatch_plan {
        ConsoleInputDispatchPlan::ListModal => return Ok(handle_list_modal(state, key)),
        ConsoleInputDispatchPlan::InlineNewSessionPicker => {
            return Ok(handle_new_session_picker(state, key));
        }
        ConsoleInputDispatchPlan::InlineProviderPicker => {
            return Ok(handle_inline_provider_picker(state, key));
        }
        ConsoleInputDispatchPlan::LaunchProviderPicker => {
            return Ok(handle_launch_provider_picker(state, key));
        }
        ConsoleInputDispatchPlan::InlineAgentPicker => {
            return Ok(handle_inline_agent_picker(state, key));
        }
        ConsoleInputDispatchPlan::InlineRolePicker => {
            return Ok(handle_inline_role_picker(state, key));
        }
        ConsoleInputDispatchPlan::EditorModal => {}
        ConsoleInputDispatchPlan::SettingsErrorPopup => {}
        ConsoleInputDispatchPlan::SettingsMountsModal => {}
        ConsoleInputDispatchPlan::SettingsEnvDialog => {}
        ConsoleInputDispatchPlan::SettingsAuthDialog => {}
        ConsoleInputDispatchPlan::CreatePreludeModal => {}
        ConsoleInputDispatchPlan::Stage(route) => {
            let outcome = match route {
                ConsoleManagerStageRoute::List => handle_list_key(state, config, paths, cwd, key),
                ConsoleManagerStageRoute::Editor => {
                    handle_editor_key(state, config, paths, cwd, key)
                }
                ConsoleManagerStageRoute::Settings => {
                    handle_settings_key_with_effects(state, key);
                    after_settings_event(state);
                    Ok(InputOutcome::Continue)
                }
                ConsoleManagerStageRoute::CreatePrelude => {
                    Ok(handle_prelude_key(state, config, paths, cwd, key))
                }
                ConsoleManagerStageRoute::ConfirmDelete => {
                    Ok(handle_confirm_delete_key(state, cwd, key))
                }
                ConsoleManagerStageRoute::ConfirmInstancePurge => {
                    Ok(handle_confirm_instance_purge_key(state, key))
                }
            }?;
            state.request_effect(ConsoleEffect::RequestActiveMountInfoRefresh.into());
            return Ok(outcome);
        }
    }

    // Capture `op_available` and the session-scoped op_cache before modal
    // borrows so commit paths can build source/op pickers.
    let op_available = state.op_available;
    let op_cache = Rc::clone(&state.op_cache);
    let term_size = state.cached_term_size;
    if matches!(dispatch_plan, ConsoleInputDispatchPlan::EditorModal) {
        let ManagerStage::Editor(editor) = &mut state.stage else {
            return Ok(InputOutcome::Continue);
        };
        let editor_outcome = handle_editor_modal(
            editor,
            key,
            op_available,
            op_cache,
            config,
            paths,
            term_size,
        );
        match editor_outcome {
            EditorModalOutcome::Continue => {}
            EditorModalOutcome::StartRoleRegistration {
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
            EditorModalOutcome::PersistTrustedRoleSource { key, source } => {
                state.request_effect(ManagerEffect::PersistTrustedRoleSource { key, source });
            }
            EditorModalOutcome::ApplyFileBrowserOutcome(outcome) => {
                state.request_effect(ManagerEffect::ApplyFileBrowserOutcome {
                    context: FileBrowserEffectContext::Editor,
                    outcome,
                });
            }
            EditorModalOutcome::ResolveFileBrowserGitUrl(path) => {
                state.request_effect(ManagerEffect::ResolveFileBrowserGitUrl(path));
            }
            EditorModalOutcome::OpenAuthSourceFolderBrowser => {
                state.request_effect(ManagerEffect::OpenEditorAuthSourceFolderBrowser);
            }
            EditorModalOutcome::OpenUrl(url) => {
                state.request_effect(ManagerEffect::OpenUrl(url));
            }
            EditorModalOutcome::ValidateOpRef(op_ref) => {
                state.request_effect(ManagerEffect::ValidateOpCommit {
                    op_ref,
                    is_settings: false,
                });
            }
        }
        state.request_effect(ConsoleEffect::RequestActiveMountInfoRefresh.into());

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
                    begin_editor_save(state, config, true)?;
                }
                ExitIntent::Discard => {
                    let _unused = update_manager(
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
    if matches!(dispatch_plan, ConsoleInputDispatchPlan::SettingsErrorPopup) {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return Ok(InputOutcome::Continue);
        };
        let dismiss = settings.error_popup.as_mut().is_some_and(|p| {
            matches!(
                dismissible_modal_plan(p.handle_key(key)),
                DismissibleModalPlan::Dismiss
            )
        });
        if dismiss {
            drop(update_manager(
                state,
                ManagerMessage::DismissSettingsErrorPopup,
            ));
        }
        return Ok(InputOutcome::Continue);
    }
    if matches!(dispatch_plan, ConsoleInputDispatchPlan::SettingsMountsModal) {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return Ok(InputOutcome::Continue);
        };
        let modal_outcome = handle_settings_confirm_modal(settings, key, term_size);
        match modal_outcome {
            SettingsModalOutcome::Continue => {}
            SettingsModalOutcome::SaveSettings => {
                state.request_effect(ConsoleEffect::SaveSettings.into());
            }
            SettingsModalOutcome::OpenGlobalMountFileBrowser => {
                state.request_effect(ManagerEffect::OpenGlobalMountFileBrowser);
            }
            SettingsModalOutcome::OpenUrl(url) => {
                state.request_effect(ManagerEffect::OpenUrl(url));
            }
            SettingsModalOutcome::ResolveFileBrowserGitUrl(path) => {
                state.request_effect(ManagerEffect::ResolveFileBrowserGitUrl(path));
            }
            SettingsModalOutcome::ApplyFileBrowserOutcome(outcome) => {
                state.request_effect(ManagerEffect::ApplyFileBrowserOutcome {
                    context: FileBrowserEffectContext::SettingsMounts,
                    outcome,
                });
            }
        }
        after_settings_event(state);
        return Ok(InputOutcome::Continue);
    }
    if matches!(dispatch_plan, ConsoleInputDispatchPlan::SettingsEnvDialog) {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return Ok(InputOutcome::Continue);
        };
        handle_settings_env_modal(&mut settings.env, key, op_cache);
        after_settings_event(state);
        return Ok(InputOutcome::Continue);
    }
    if matches!(dispatch_plan, ConsoleInputDispatchPlan::SettingsAuthDialog) {
        let ManagerStage::Settings(settings) = &mut state.stage else {
            return Ok(InputOutcome::Continue);
        };
        let auth_outcome = handle_settings_auth_modal(
            &mut settings.auth,
            &mut settings.env,
            &mut settings.pending_token_generate,
            key,
            op_available,
            op_cache,
            term_size,
            validate_auth_source_folder,
        );
        match auth_outcome {
            SettingsAuthOutcome::Continue => {}
            SettingsAuthOutcome::OpenAuthSourceFolderBrowser => {
                state.request_effect(ManagerEffect::OpenSettingsAuthSourceFolderBrowser);
            }
            SettingsAuthOutcome::ApplyFileBrowserOutcome(outcome) => {
                state.request_effect(ManagerEffect::ApplyFileBrowserOutcome {
                    context: FileBrowserEffectContext::SettingsAuth,
                    outcome,
                });
            }
            SettingsAuthOutcome::ValidateOpRef(op_ref) => {
                state.request_effect(ManagerEffect::ValidateOpCommit {
                    op_ref,
                    is_settings: true,
                });
            }
        }
        after_settings_event(state);
        return Ok(InputOutcome::Continue);
    }
    if matches!(dispatch_plan, ConsoleInputDispatchPlan::CreatePreludeModal) {
        let outcome = if let ManagerStage::CreatePrelude(p) = &mut state.stage {
            handle_prelude_modal(p, key, term_size)
        } else {
            PreludeModalOutcome::Continue
        };
        match outcome {
            PreludeModalOutcome::Continue => {}
            PreludeModalOutcome::OpenUrl(url) => {
                state.request_effect(ManagerEffect::OpenUrl(url));
                return Ok(InputOutcome::Continue);
            }
            PreludeModalOutcome::ReopenFileBrowserAtLastCwd => {
                state.request_effect(ManagerEffect::OpenCreatePreludeFileBrowserAtLastCwd);
                return Ok(InputOutcome::Continue);
            }
            PreludeModalOutcome::ResolveFileBrowserGitUrl(path) => {
                state.request_effect(ManagerEffect::ResolveFileBrowserGitUrl(path));
                return Ok(InputOutcome::Continue);
            }
            PreludeModalOutcome::ApplyFileBrowserOutcome {
                outcome,
                browser_cwd,
            } => {
                state.request_effect(ManagerEffect::ApplyFileBrowserOutcome {
                    context: FileBrowserEffectContext::Prelude { browser_cwd },
                    outcome,
                });
                return Ok(InputOutcome::Continue);
            }
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
        let (status, completed) = if let ManagerStage::CreatePrelude(p) = &state.stage {
            let completed = p.completed().map(Box::new);
            (
                create_prelude_completion_status(p.modal.is_some(), completed.is_some()),
                completed,
            )
        } else {
            (CreatePreludeCompletionStatus::InProgress, None)
        };
        match status {
            CreatePreludeCompletionStatus::Complete => {
                let Some(payload) = completed else {
                    return Ok(InputOutcome::Continue);
                };
                let (name, ws) = *payload;
                let _unused = update_manager(
                    state,
                    ManagerMessage::EnterCreateEditor {
                        name,
                        workspace: ws,
                    },
                );
            }
            CreatePreludeCompletionStatus::Cancelled => {
                let _unused = update_manager(
                    state,
                    ManagerMessage::ReloadFromConfig {
                        config: Box::new(config.clone()),
                        cwd: cwd.to_path_buf(),
                    },
                );
            }
            CreatePreludeCompletionStatus::InProgress => {}
        }
        return Ok(InputOutcome::Continue);
    }

    Ok(InputOutcome::Continue)
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
    let plan = instance_purge_key_plan(confirm_state.handle_key(key), container.clone());
    match plan {
        InstancePurgeKeyPlan::Purge { container } => {
            drop(update_manager(state, ManagerMessage::ReturnToList));
            InputOutcome::InstanceAction {
                container,
                action: crate::tui::message::ConsoleInstanceAction::Purge,
            }
        }
        InstancePurgeKeyPlan::ReturnToList => {
            drop(update_manager(state, ManagerMessage::ReturnToList));
            InputOutcome::Continue
        }
        InstancePurgeKeyPlan::Continue => InputOutcome::Continue,
    }
}

fn handle_confirm_delete_key(
    state: &mut ManagerState<'_>,
    cwd: &std::path::Path,
    key: KeyEvent,
) -> InputOutcome {
    let ManagerStage::ConfirmDelete {
        name,
        state: confirm_state,
    } = &mut state.stage
    else {
        return InputOutcome::Continue;
    };
    let plan = workspace_delete_key_plan(confirm_state.handle_key(key), name.clone());
    match plan {
        WorkspaceDeleteKeyPlan::RemoveWorkspace { name } => {
            drop(update_manager(state, ManagerMessage::ReturnToList));
            state.request_effect(ManagerEffect::RemoveWorkspace {
                name,
                cwd: cwd.to_path_buf(),
            });
            InputOutcome::Continue
        }
        WorkspaceDeleteKeyPlan::ReturnToList => {
            drop(update_manager(state, ManagerMessage::ReturnToList));
            InputOutcome::Continue
        }
        WorkspaceDeleteKeyPlan::Continue => InputOutcome::Continue,
    }
}
