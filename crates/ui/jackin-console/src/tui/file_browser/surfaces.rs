// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Per-surface file-browser outcome executors.

use crate::tui::components::file_browser::{FileBrowserOutcome, FileBrowserState, FolderListing};
use crate::tui::effect::FileBrowserEffectContext;

use crate::tui::state::{ManagerStage, ManagerState, Modal, SettingsModal};

pub(crate) fn execute_editor_file_browser_outcome(
    state: &mut ManagerState<'_>,
    outcome: FileBrowserOutcome<std::path::PathBuf>,
    auth_source_folder_validator: &impl Fn(
        Option<crate::tui::auth::AuthKind>,
        &std::path::Path,
    ) -> Result<(), String>,
) -> bool {
    use crate::tui::components::error_popup;
    use crate::tui::state::FileBrowserTarget;
    let ManagerStage::Editor(editor) = &mut state.stage else {
        return false;
    };
    let target = {
        let Some(Modal::FileBrowser { target, .. }) = editor.modal.as_mut() else {
            return false;
        };
        target.clone()
    };
    match outcome {
        FileBrowserOutcome::Commit(path) => {
            // Auth source-folder picks must hold the selected agent's
            // credential structure. Reject a wrong folder inline and keep
            // the picker open rather than saving an unusable path.
            if target == FileBrowserTarget::AuthFormSourceFolder
                && let Err(reason) = auth_source_folder_validator(
                    editor
                        .modal_parents
                        .iter()
                        .rev()
                        .find_map(|modal| match modal {
                            Modal::AuthForm { state, .. } => Some(state.kind),
                            _ => None,
                        }),
                    &path,
                )
            {
                editor.open_sub_modal(Modal::ErrorPopup {
                    state: error_popup::invalid_source_folder_error_popup_state(reason),
                });
                return true;
            }
            crate::tui::input::editor::apply_file_browser_to_editor(target, editor, path);
        }
        FileBrowserOutcome::Cancel => editor.pop_modal_chain(),
        FileBrowserOutcome::Continue
        | FileBrowserOutcome::OpenGitUrl(_)
        | FileBrowserOutcome::ResolveGitUrl(_)
        | FileBrowserOutcome::NavigateTo(_)
        | FileBrowserOutcome::NavigateUp
        | FileBrowserOutcome::RequestCommit(_) => {}
    }
    true
}

pub(crate) fn execute_prelude_file_browser_outcome(
    state: &mut ManagerState<'_>,
    outcome: FileBrowserOutcome<std::path::PathBuf>,
    browser_cwd: Option<std::path::PathBuf>,
) -> bool {
    use crate::tui::state::FileBrowserTarget;
    let ManagerStage::CreatePrelude(prelude) = &mut state.stage else {
        return false;
    };
    if !matches!(prelude.modal, Some(Modal::FileBrowser { .. })) {
        return false;
    }
    match outcome {
        FileBrowserOutcome::Commit(path) => {
            prelude.modal = None;
            prelude.last_browser_cwd = browser_cwd;
            prelude.accept_mount_src(path);
            // FileBrowser commit advances the wizard to `mount-dst-choice`.
            prelude.wizard.next();
            let src = prelude
                .pending_mount_src
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            prelude.modal = Some(Modal::MountDstChoice {
                target: FileBrowserTarget::CreateFirstMountSrc,
                state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src),
            });
        }
        FileBrowserOutcome::Cancel => {
            prelude.wizard.cancel();
            prelude.modal = None;
        }
        FileBrowserOutcome::Continue
        | FileBrowserOutcome::OpenGitUrl(_)
        | FileBrowserOutcome::ResolveGitUrl(_)
        | FileBrowserOutcome::NavigateTo(_)
        | FileBrowserOutcome::NavigateUp
        | FileBrowserOutcome::RequestCommit(_) => {}
    }
    true
}

pub(crate) fn execute_settings_file_browser_outcome(
    state: &mut ManagerState<'_>,
    outcome: FileBrowserOutcome<std::path::PathBuf>,
) -> bool {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return false;
    };
    if !matches!(
        settings.mounts.modals.current(),
        Some(SettingsModal::MountFileBrowser { .. })
    ) {
        return false;
    }
    match outcome {
        FileBrowserOutcome::Commit(path) => {
            let src = path.display().to_string();
            if let Some(draft) = settings.mounts.add_draft.as_mut() {
                draft.src.clone_from(&src);
            }
            settings
                .mounts
                .open_sub_modal(SettingsModal::MountDstChoice {
                    state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src),
                });
        }
        FileBrowserOutcome::Cancel => {
            settings.mounts.pop_modal_chain();
            if !settings.mounts.modals.is_open() {
                settings.mounts.add_draft = None;
            }
        }
        FileBrowserOutcome::Continue
        | FileBrowserOutcome::OpenGitUrl(_)
        | FileBrowserOutcome::ResolveGitUrl(_)
        | FileBrowserOutcome::NavigateTo(_)
        | FileBrowserOutcome::NavigateUp
        | FileBrowserOutcome::RequestCommit(_) => {}
    }
    true
}

pub(crate) fn apply_file_browser_listing(
    state: &mut ManagerState<'_>,
    context: &FileBrowserEffectContext,
    listing: Option<FolderListing>,
) -> bool {
    let Some(listing) = listing else {
        return false;
    };
    let Some(browser) = active_file_browser_state_mut(state, context) else {
        return false;
    };
    browser.apply_listing(listing);
    true
}

pub(crate) fn active_file_browser_state_mut<'a>(
    state: &'a mut ManagerState<'_>,
    context: &FileBrowserEffectContext,
) -> Option<&'a mut FileBrowserState> {
    match context {
        FileBrowserEffectContext::Editor => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return None;
            };
            let Some(Modal::FileBrowser { state, .. }) = editor.modal.as_mut() else {
                return None;
            };
            Some(state)
        }
        FileBrowserEffectContext::Prelude { .. } => {
            let ManagerStage::CreatePrelude(prelude) = &mut state.stage else {
                return None;
            };
            let Some(Modal::FileBrowser { state, .. }) = prelude.modal.as_mut() else {
                return None;
            };
            Some(state)
        }
        FileBrowserEffectContext::SettingsMounts => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return None;
            };
            let Some(SettingsModal::MountFileBrowser { state }) =
                settings.mounts.modals.current_mut()
            else {
                return None;
            };
            Some(state)
        }
        FileBrowserEffectContext::SettingsAuth => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return None;
            };
            let Some(SettingsModal::AuthSourceFolderPicker { state }) =
                settings.auth.modals.current_mut()
            else {
                return None;
            };
            Some(state)
        }
    }
}
