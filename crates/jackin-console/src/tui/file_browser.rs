// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! File-browser effect executors: open, apply outcome, resolve git URL, poll.
//!
//! `execute_editor_file_browser_outcome` accepts an injectable
//! `auth_source_folder_validator` so the root binary can supply the macOS
//! `security`-subprocess check without creating a runtime dep here.

use crate::services::file_browser::{
    FileBrowserListingRequest, FileBrowserListingResult, FileBrowserOpenTarget,
};
use crate::tui::components::file_browser::{FileBrowserOutcome, FileBrowserState, FolderListing};
use crate::tui::effect::FileBrowserEffectContext;
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::state::{
    AuthFormFocus, CreatePreludeState, FileBrowserTarget, ManagerStage, ManagerState, Modal,
    SettingsModal,
};

pub type AuthSourceFolderValidator =
    fn(Option<crate::tui::auth::AuthKind>, &std::path::Path) -> Result<(), String>;

#[derive(Debug)]
pub enum FileBrowserCommitResult {
    Accepted {
        context: FileBrowserEffectContext,
        path: std::path::PathBuf,
    },
    Rejected {
        context: FileBrowserEffectContext,
        reason: String,
    },
}

pub fn start_global_mount_file_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Settings(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::GlobalMount,
            last_cwd: None,
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_editor_add_mount_file_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Editor(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::EditorAddMount,
            last_cwd: None,
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_editor_auth_source_folder_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Editor(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::EditorAuthSourceFolder,
            last_cwd: None,
            show_hidden: true,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_create_prelude_file_browser_open(state: &mut ManagerState<'_>) -> bool {
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::CreatePrelude,
            last_cwd: None,
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_settings_auth_source_folder_browser_open(state: &mut ManagerState<'_>) -> bool {
    if !matches!(state.stage, ManagerStage::Settings(_)) {
        return false;
    }
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::SettingsAuthSourceFolder,
            last_cwd: None,
            show_hidden: true,
        });
    state.begin_file_browser_listing(rx);
    true
}

pub fn start_create_prelude_file_browser_reopen(state: &mut ManagerState<'_>) -> bool {
    let ManagerStage::CreatePrelude(prelude) = &mut state.stage else {
        return false;
    };
    let rx =
        crate::services::file_browser::start_listing_request(FileBrowserListingRequest::OpenHome {
            target: FileBrowserOpenTarget::CreatePrelude,
            last_cwd: prelude.last_browser_cwd.clone(),
            show_hidden: false,
        });
    state.begin_file_browser_listing(rx);
    true
}

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

fn apply_file_browser_open_result(
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
                drop(update_manager(
                    state,
                    ManagerMessage::EnterCreatePrelude(prelude),
                ));
            }
            Err(error) => {
                let _unused = update_manager(
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

enum FileBrowserListingRequestKind {
    NavigateTo(std::path::PathBuf),
    NavigateUp,
}

fn start_file_browser_listing_for_navigation(
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
    let rx = jackin_tui::runtime::spawn_named_blocking_subscription(
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

fn active_file_browser_commit_facts(
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
                editor.auth_selected_kind
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
                settings.auth.modal.as_mut()
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

fn apply_file_browser_commit(
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

fn apply_file_browser_rejection(
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

fn execute_editor_file_browser_outcome(
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
                && let Err(reason) = auth_source_folder_validator(editor.auth_selected_kind, &path)
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

fn execute_prelude_file_browser_outcome(
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

fn execute_settings_file_browser_outcome(
    state: &mut ManagerState<'_>,
    outcome: FileBrowserOutcome<std::path::PathBuf>,
) -> bool {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return false;
    };
    if !matches!(
        settings.mounts.modal,
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
            if settings.mounts.modal.is_none() {
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

fn apply_file_browser_listing(
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

fn active_file_browser_state_mut<'a>(
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
            let Some(SettingsModal::MountFileBrowser { state }) = settings.mounts.modal.as_mut()
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
                settings.auth.modal.as_mut()
            else {
                return None;
            };
            Some(state)
        }
    }
}

#[allow(
    clippy::option_if_let_else,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn execute_file_browser_git_url_resolution(
    state: &mut ManagerState<'_>,
    path: &std::path::Path,
) -> bool {
    if let Some(modal) = state.list_modal.as_mut()
        && attach_modal_file_browser_git_url(modal, path.to_owned())
    {
        return true;
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = editor.modal.as_mut()
                && attach_modal_file_browser_git_url(modal, path.to_owned())
            {
                return true;
            }
            for modal in &mut editor.modal_parents {
                if attach_modal_file_browser_git_url(modal, path.to_owned()) {
                    return true;
                }
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = prelude.modal.as_mut()
                && attach_modal_file_browser_git_url(modal, path.to_owned())
            {
                return true;
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modal.as_mut()
                && attach_global_mount_file_browser_git_url(modal, path.to_owned())
            {
                return true;
            }
            for modal in &mut settings.mounts.modal_parents {
                if attach_global_mount_file_browser_git_url(modal, path.to_owned()) {
                    return true;
                }
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    false
}

fn attach_modal_file_browser_git_url(modal: &mut Modal<'_>, path: std::path::PathBuf) -> bool {
    match modal {
        Modal::FileBrowser { state, .. } => {
            crate::services::file_browser::request_git_url_resolution(state, path);
            true
        }
        _ => false,
    }
}

fn attach_global_mount_file_browser_git_url(
    modal: &mut SettingsModal<'_>,
    path: std::path::PathBuf,
) -> bool {
    match modal {
        SettingsModal::MountFileBrowser { state } => {
            crate::services::file_browser::request_git_url_resolution(state, path);
            true
        }
        _ => false,
    }
}

pub fn poll_file_browser_git_urls(state: &mut ManagerState<'_>) -> bool {
    let mut dirty = false;
    if let Some(modal) = state.list_modal.as_mut() {
        dirty |= poll_modal_file_browser_git_url(modal);
    }
    match &mut state.stage {
        ManagerStage::Editor(editor) => {
            if let Some(modal) = editor.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
            for modal in &mut editor.modal_parents {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::CreatePrelude(prelude) => {
            if let Some(modal) = prelude.modal.as_mut() {
                dirty |= poll_modal_file_browser_git_url(modal);
            }
        }
        ManagerStage::Settings(settings) => {
            if let Some(modal) = settings.mounts.modal.as_mut() {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
            for modal in &mut settings.mounts.modal_parents {
                dirty |= poll_global_mount_file_browser_git_url(modal);
            }
        }
        ManagerStage::List
        | ManagerStage::ConfirmDelete { .. }
        | ManagerStage::ConfirmInstancePurge { .. } => {}
    }
    dirty
}

fn poll_modal_file_browser_git_url(modal: &mut Modal<'_>) -> bool {
    match modal {
        Modal::FileBrowser { state, .. } => state.poll_git_url_resolution(),
        _ => false,
    }
}

fn poll_global_mount_file_browser_git_url(modal: &mut SettingsModal<'_>) -> bool {
    match modal {
        SettingsModal::MountFileBrowser { state } => state.poll_git_url_resolution(),
        _ => false,
    }
}
