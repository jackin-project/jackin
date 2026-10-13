// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! File-browser commit validation and rejection.

use super::{AuthSourceFolderValidator, FileBrowserCommitResult, active_file_browser_state_mut};
use crate::services::file_browser::FileBrowserListingRequest;

use crate::tui::effect::FileBrowserEffectContext;

use crate::tui::state::{FileBrowserTarget, ManagerStage, ManagerState, Modal, SettingsModal};

pub(crate) enum FileBrowserListingRequestKind {
    NavigateTo(std::path::PathBuf),
    NavigateUp,
}

pub(crate) fn start_file_browser_listing_for_navigation(
    state: &mut ManagerState<'_>,
    context: FileBrowserEffectContext,
    kind: FileBrowserListingRequestKind,
) -> bool {
    let Some(browser) = active_file_browser_state_mut(state, &context) else {
        return false;
    };
    let root = browser.root.clone();
    let cwd = browser.cwd().to_path_buf();
    let show_hidden = browser.show_hidden;
    let request = match kind {
        FileBrowserListingRequestKind::NavigateTo(path) => FileBrowserListingRequest::NavigateTo {
            context,
            root,
            path,
            show_hidden,
        },
        FileBrowserListingRequestKind::NavigateUp => FileBrowserListingRequest::NavigateUp {
            context,
            root,
            cwd,
            show_hidden,
        },
    };
    let rx = crate::services::file_browser::start_listing_request(request);
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_file_browser_commit_validation(
    state: &mut ManagerState<'_>,
    context: FileBrowserEffectContext,
    path: std::path::PathBuf,
    auth_source_folder_validator: AuthSourceFolderValidator,
) -> bool {
    let Some((root, auth_kind)) = active_file_browser_commit_facts(state, &context) else {
        return false;
    };
    let worker_context = context.clone();
    let rx = crate::tui::runtime::spawn_named_blocking_subscription(
        "jackin-file-browser-commit",
        move || {
            let path = match crate::services::file_browser::validate_commit(&root, &path) {
                Ok(path) => path,
                Err(reason) => {
                    return FileBrowserCommitResult::Rejected {
                        context: worker_context,
                        reason,
                    };
                }
            };
            if let Some(kind) = auth_kind
                && let Err(reason) = auth_source_folder_validator(Some(kind), &path)
            {
                return FileBrowserCommitResult::Rejected {
                    context: worker_context,
                    reason,
                };
            }
            FileBrowserCommitResult::Accepted {
                context: worker_context,
                path,
            }
        },
    );
    state.begin_file_browser_commit(rx);
    true
}

pub(crate) fn active_file_browser_commit_facts(
    state: &mut ManagerState<'_>,
    context: &FileBrowserEffectContext,
) -> Option<(std::path::PathBuf, Option<crate::tui::auth::AuthKind>)> {
    match context {
        FileBrowserEffectContext::Editor => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return None;
            };
            let Some(Modal::FileBrowser {
                target,
                state: browser,
            }) = editor.modal.as_mut()
            else {
                return None;
            };
            let auth_kind = if *target == FileBrowserTarget::AuthFormSourceFolder {
                editor
                    .modal_parents
                    .iter()
                    .rev()
                    .find_map(|modal| match modal {
                        Modal::AuthForm { state, .. } => Some(state.kind),
                        _ => None,
                    })
            } else {
                None
            };
            Some((browser.root.clone(), auth_kind))
        }
        FileBrowserEffectContext::Prelude { .. } | FileBrowserEffectContext::SettingsMounts => {
            let browser = active_file_browser_state_mut(state, context)?;
            Some((browser.root.clone(), None))
        }
        FileBrowserEffectContext::SettingsAuth => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return None;
            };
            let Some(SettingsModal::AuthSourceFolderPicker { state: browser }) =
                settings.auth.modals.current_mut()
            else {
                return None;
            };
            Some((browser.root.clone(), settings.auth.selected_kind()))
        }
    }
}

pub fn apply_file_browser_commit_result(
    state: &mut ManagerState<'_>,
    result: FileBrowserCommitResult,
) -> bool {
    match result {
        FileBrowserCommitResult::Accepted { context, path } => {
            apply_file_browser_commit(state, context, path)
        }
        FileBrowserCommitResult::Rejected { context, reason } => {
            apply_file_browser_rejection(state, &context, reason)
        }
    }
}

pub(crate) fn apply_file_browser_commit(
    state: &mut ManagerState<'_>,
    context: FileBrowserEffectContext,
    path: std::path::PathBuf,
) -> bool {
    match context {
        FileBrowserEffectContext::Editor => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return false;
            };
            let Some(Modal::FileBrowser { target, .. }) = editor.modal.as_ref() else {
                return false;
            };
            crate::tui::input::editor::apply_file_browser_to_editor(target.clone(), editor, path);
            true
        }
        FileBrowserEffectContext::Prelude { browser_cwd } => {
            use crate::tui::state::FileBrowserTarget;
            let ManagerStage::CreatePrelude(prelude) = &mut state.stage else {
                return false;
            };
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
            true
        }
        FileBrowserEffectContext::SettingsMounts => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return false;
            };
            let src = path.display().to_string();
            if let Some(draft) = settings.mounts.add_draft.as_mut() {
                draft.src.clone_from(&src);
            }
            settings
                .mounts
                .open_sub_modal(SettingsModal::MountDstChoice {
                    state: crate::tui::components::mount_dst_choice::MountDstChoiceState::new(src),
                });
            true
        }
        FileBrowserEffectContext::SettingsAuth => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return false;
            };
            crate::tui::input::global_mounts::apply_source_folder_to_settings_auth_form(
                &mut settings.auth,
                path,
            );
            true
        }
    }
}

pub(crate) fn apply_file_browser_rejection(
    state: &mut ManagerState<'_>,
    context: &FileBrowserEffectContext,
    reason: String,
) -> bool {
    match context {
        FileBrowserEffectContext::Editor => {
            let ManagerStage::Editor(editor) = &mut state.stage else {
                return false;
            };
            let Some(Modal::FileBrowser { target, state }) = editor.modal.as_mut() else {
                return false;
            };
            if *target == FileBrowserTarget::AuthFormSourceFolder {
                editor.open_sub_modal(Modal::ErrorPopup {
                    state: crate::tui::components::error_popup::invalid_source_folder_error_popup_state(
                        reason,
                    ),
                });
            } else {
                state.reject_commit(reason);
            }
            true
        }
        FileBrowserEffectContext::SettingsAuth => {
            let ManagerStage::Settings(settings) = &mut state.stage else {
                return false;
            };
            settings.auth.set_error(reason);
            true
        }
        FileBrowserEffectContext::Prelude { .. } | FileBrowserEffectContext::SettingsMounts => {
            let Some(browser) = active_file_browser_state_mut(state, context) else {
                return false;
            };
            browser.reject_commit(reason);
            true
        }
    }
}
