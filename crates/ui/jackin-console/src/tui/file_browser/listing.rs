// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! File-browser listing results and outcome execution.

use super::{
    AuthSourceFolderValidator, FileBrowserListingRequestKind, apply_file_browser_listing,
    execute_editor_file_browser_outcome, execute_prelude_file_browser_outcome,
    execute_settings_file_browser_outcome, start_file_browser_commit_validation,
    start_file_browser_listing_for_navigation,
};
use crate::services::file_browser::{FileBrowserListingResult, FileBrowserOpenTarget};
use crate::tui::components::file_browser::{FileBrowserOutcome, FileBrowserState};
use crate::tui::effect::FileBrowserEffectContext;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    AuthFormFocus, CreatePreludeState, FileBrowserTarget, ManagerStage, ManagerState, Modal,
    SettingsModal,
};

pub fn apply_file_browser_listing_result(
    state: &mut ManagerState<'_>,
    result: FileBrowserListingResult,
) -> bool {
    match result {
        FileBrowserListingResult::OpenHome { target, result } => {
            apply_file_browser_open_result(state, target, result)
        }
        FileBrowserListingResult::Listing { context, listing } => {
            apply_file_browser_listing(state, &context, listing)
        }
    }
}

pub(crate) fn apply_file_browser_open_result(
    state: &mut ManagerState<'_>,
    target: FileBrowserOpenTarget,
    result: Result<Box<FileBrowserState>, String>,
) -> bool {
    use crate::tui::components::error_popup;
    match target {
        FileBrowserOpenTarget::EditorAddMount => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return false;
            };
            match result {
                Ok(file_browser) => {
                    editor.modal = Some(Modal::FileBrowser {
                        target: FileBrowserTarget::EditAddMountSrc,
                        state: *file_browser,
                    });
                }
                Err(error) => {
                    crate::tui::state::open_editor_action_error(editor, &anyhow::anyhow!(error));
                }
            }
        }
        FileBrowserOpenTarget::EditorAuthSourceFolder => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return false;
            };
            match result {
                Ok(file_browser) => {
                    crate::tui::input::auth::open_auth_source_folder_browser_from_form_with_state(
                        editor,
                        *file_browser,
                    )
                }
                Err(error) => {
                    crate::tui::state::open_editor_action_error(editor, &anyhow::anyhow!(error));
                    true
                }
            };
        }
        FileBrowserOpenTarget::CreatePrelude => match result {
            Ok(file_browser) => {
                let mut prelude = CreatePreludeState::new();
                prelude.modal = Some(Modal::FileBrowser {
                    target: FileBrowserTarget::CreateFirstMountSrc,
                    state: *file_browser,
                });
                update_manager(state, ManagerMessage::EnterCreatePrelude(prelude));
            }
            Err(error) => {
                update_manager(
                    state,
                    ManagerMessage::OpenListErrorPopup {
                        title: error_popup::file_browser_failed_error_title().into(),
                        message: error,
                    },
                );
            }
        },
        FileBrowserOpenTarget::GlobalMount => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return false;
            };
            match result {
                Ok(file_browser) => {
                    settings
                        .mounts
                        .open_sub_modal(SettingsModal::MountFileBrowser {
                            state: file_browser,
                        });
                }
                Err(error) => {
                    settings.mounts.add_draft = None;
                    settings.mounts.error = Some(error);
                }
            }
        }
        FileBrowserOpenTarget::SettingsAuthSourceFolder => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return false;
            };
            match result {
                Ok(file_browser) => {
                    let Some(SettingsModal::AuthForm {
                        target,
                        state,
                        focus,
                        literal_buffer,
                    }) = settings.auth.take_modal()
                    else {
                        return false;
                    };
                    if !state.shows_source_folder() {
                        settings.auth.set_modal(SettingsModal::AuthForm {
                            target,
                            state,
                            focus,
                            literal_buffer,
                        });
                        return false;
                    }
                    settings.auth.open_child_modal(
                        SettingsModal::AuthForm {
                            target,
                            state,
                            focus: AuthFormFocus::SourceFolder,
                            literal_buffer,
                        },
                        SettingsModal::AuthSourceFolderPicker {
                            state: *file_browser,
                        },
                    );
                }
                Err(error) => settings.auth.set_error(error),
            }
        }
    }
    true
}

/// Dispatch a file-browser outcome to the active stage handler.
///
/// `auth_source_folder_validator` is injected so the root binary can provide
/// the runtime check without creating a dependency on the runtime crate here.
pub fn execute_file_browser_outcome(
    state: &mut ManagerState<'_>,
    context: FileBrowserEffectContext,
    outcome: FileBrowserOutcome<std::path::PathBuf>,
    auth_source_folder_validator: &impl Fn(
        Option<crate::tui::auth::AuthKind>,
        &std::path::Path,
    ) -> Result<(), String>,
) -> bool {
    match context {
        FileBrowserEffectContext::Editor => {
            execute_editor_file_browser_outcome(state, outcome, auth_source_folder_validator)
        }
        FileBrowserEffectContext::Prelude { browser_cwd } => {
            execute_prelude_file_browser_outcome(state, outcome, browser_cwd)
        }
        FileBrowserEffectContext::SettingsMounts => {
            execute_settings_file_browser_outcome(state, outcome)
        }
        FileBrowserEffectContext::SettingsAuth => false,
    }
}

pub fn execute_file_browser_outcome_or_start_listing(
    state: &mut ManagerState<'_>,
    context: FileBrowserEffectContext,
    outcome: FileBrowserOutcome<std::path::PathBuf>,
    auth_source_folder_validator: AuthSourceFolderValidator,
) -> bool {
    match outcome {
        FileBrowserOutcome::NavigateTo(path) => start_file_browser_listing_for_navigation(
            state,
            context.clone(),
            FileBrowserListingRequestKind::NavigateTo(path),
        ),
        FileBrowserOutcome::NavigateUp => start_file_browser_listing_for_navigation(
            state,
            context.clone(),
            FileBrowserListingRequestKind::NavigateUp,
        ),
        FileBrowserOutcome::RequestCommit(path) => start_file_browser_commit_validation(
            state,
            context.clone(),
            path,
            auth_source_folder_validator,
        ),
        outcome => {
            execute_file_browser_outcome(state, context, outcome, &auth_source_folder_validator)
        }
    }
}
