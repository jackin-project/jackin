// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings auth modal handling.

use super::{
    SourceFolderValidator, apply_source_folder_to_settings_auth_form, clear_settings_auth_kind,
    commit_settings_auth_text, persist_settings_auth_form, restore_settings_auth_form,
};

use crate::tui::components::auth_panel::AuthFormKeyPlan;

use crossterm::event::KeyCode;
use crossterm::event::KeyEvent;

use super::super::SettingsAuthOutcome;
use crate::tui::components::auth_panel::auth_credential_input_state;
use crate::tui::components::auth_panel::auth_form_key_plan_with_source_folder;
use crate::tui::components::auth_panel::auth_source_picker_state;
use crate::tui::state::SettingsModal;

use crate::tui::components::file_browser::page_rows_for_modal;
use crate::tui::update::{
    AuthSourceFolderPickerPlan, InlinePickerPlan, SourcePickerPlan, auth_source_folder_picker_plan,
    inline_picker_plan, source_picker_plan,
};

pub fn handle_settings_auth_modal(
    auth: &mut crate::tui::state::SettingsAuthState,
    env: &mut crate::tui::state::SettingsEnvState<'_>,
    key: KeyEvent,
    op_available: bool,
    op_cache: std::rc::Rc<std::cell::RefCell<jackin_env::OpCache>>,
    term_size: ratatui::layout::Rect,
    validate_source_folder: &SourceFolderValidator,
) -> SettingsAuthOutcome {
    let Some(mut modal) = auth.take_modal() else {
        return SettingsAuthOutcome::Continue;
    };
    match &mut modal {
        SettingsModal::AuthForm {
            target,
            state,
            focus,
            literal_buffer: _,
        } => {
            if key.code == KeyCode::Esc {
                return SettingsAuthOutcome::Continue;
            }
            let plan = auth_form_key_plan_with_source_folder(
                *focus,
                key.code,
                state.shows_source_folder(),
                state.shows_credential_block(),
                state.can_save(),
            );
            match plan {
                AuthFormKeyPlan::Stay => {}
                AuthFormKeyPlan::Focus(next) => *focus = next,
                AuthFormKeyPlan::CycleMode => state.cycle_mode(),
                AuthFormKeyPlan::OpenCredentialSource => {
                    let Some(env_var) = state.mode.and_then(|m| state.kind.required_env_var(m))
                    else {
                        auth.set_modal(modal);
                        return SettingsAuthOutcome::Continue;
                    };
                    auth.open_child_modal(
                        modal,
                        SettingsModal::AuthSourcePicker {
                            state: auth_source_picker_state(env_var, op_available),
                        },
                    );
                    return SettingsAuthOutcome::Continue;
                }
                AuthFormKeyPlan::OpenSourceFolderBrowser => {
                    auth.set_modal(modal);
                    return SettingsAuthOutcome::OpenAuthSourceFolderBrowser;
                }
                AuthFormKeyPlan::Save => {
                    persist_settings_auth_form(auth, env, state);
                    return SettingsAuthOutcome::Continue;
                }
                AuthFormKeyPlan::Cancel => return SettingsAuthOutcome::Continue,
                AuthFormKeyPlan::Reset => {
                    clear_settings_auth_kind(auth, env, target);
                    return SettingsAuthOutcome::Continue;
                }
            }
            auth.set_modal(modal);
        }
        SettingsModal::AuthSourcePicker { state } => {
            let outcome = state.handle_key(key);
            match source_picker_plan(outcome) {
                SourcePickerPlan::Plain => {
                    let literal = auth
                        .modals
                        .parents()
                        .last()
                        .and_then(|m| {
                            if let SettingsModal::AuthForm { literal_buffer, .. } = m {
                                Some(literal_buffer.clone())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_default();
                    auth.set_modal(SettingsModal::AuthTextInput {
                        state: Box::new(auth_credential_input_state(literal)),
                    });
                }
                SourcePickerPlan::Op => {
                    auth.set_modal(SettingsModal::AuthOpPicker {
                        state: Box::new(crate::tui::op_picker::OpPickerState::new_with_cache(
                            op_cache,
                        )),
                    });
                }
                SourcePickerPlan::Dismiss => restore_settings_auth_form(auth),
                SourcePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        SettingsModal::AuthTextInput { state } => {
            match inline_picker_plan(state.handle_key(key.into())) {
                InlinePickerPlan::Commit(value) => {
                    if let Err(error) = commit_settings_auth_text(auth, value) {
                        auth.set_error(error);
                        auth.set_modal(modal);
                    }
                }
                InlinePickerPlan::Dismiss => restore_settings_auth_form(auth),
                InlinePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        SettingsModal::AuthSourceFolderPicker { state } => {
            let page_rows = page_rows_for_modal(term_size, state);
            let browser_outcome = state.handle_key_with_page_rows(key, Some(page_rows));
            match browser_outcome {
                crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
                | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
                | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
                    auth.set_modal(modal);
                    return SettingsAuthOutcome::ApplyFileBrowserOutcome(browser_outcome);
                }
                other => {
                    match auth_source_folder_picker_plan(other) {
                        AuthSourceFolderPickerPlan::Commit(path) => {
                            match validate_source_folder(auth.selected_kind(), &path) {
                                Ok(()) => apply_source_folder_to_settings_auth_form(auth, path),
                                // Wrong folder for this agent: keep the picker open and
                                // raise the standard error dialog (promoted from
                                // `auth.error`) over it, rather than committing a folder
                                // that yields no credentials. Dismissing the dialog
                                // leaves the picker so the operator can pick another.
                                Err(reason) => {
                                    auth.set_error(reason);
                                    auth.set_modal(modal);
                                }
                            }
                        }
                        AuthSourceFolderPickerPlan::Close => restore_settings_auth_form(auth),
                        AuthSourceFolderPickerPlan::KeepModal => {
                            auth.set_modal(modal);
                        }
                    }
                }
            }
        }
        SettingsModal::AuthOpPicker { state } => {
            let outcome = state.handle_key(key);
            match crate::tui::update::op_picker_inline_plan(outcome) {
                // Browse-mode caller: only `Existing` is reachable.
                InlinePickerPlan::Commit(
                    crate::tui::op_picker::OpPickerSelection::NewItem { .. }
                    | crate::tui::op_picker::OpPickerSelection::EditItemField { .. },
                ) => unreachable!("settings-auth browse OpPicker runs in Browse mode"),
                InlinePickerPlan::Commit(crate::tui::op_picker::OpPickerSelection::Existing(
                    op_ref,
                )) => {
                    // Close the OpPicker — the auth form stays stashed on
                    // modal_parents so the _committed / _failed helpers find it.
                    // Dispatch already took the current picker. Preserve its
                    // parent until asynchronous validation completes.
                    return SettingsAuthOutcome::ValidateOpRef(op_ref);
                }
                InlinePickerPlan::Dismiss => restore_settings_auth_form(auth),
                InlinePickerPlan::Continue => auth.set_modal(modal),
            }
        }
        _ => unreachable!("auth input handler received a non-auth settings modal"),
    }
    SettingsAuthOutcome::Continue
}
