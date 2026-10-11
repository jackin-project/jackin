// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Inline and browser modal plans.

use super::{
    AuthSourceFolderPickerPlan, FileBrowserModalPlan, InlineAccountFollowupPlan, InlinePickerPlan,
    InlinePickerShellPlan, MountDstChoicePlan,
};
use crate::tui::components::account_picker::AccountPickerState;
use crossterm::event::KeyEvent;

#[must_use]
pub fn inline_account_followup_plan<C, A, P>(
    context: C,
    agent: A,
    providers: Vec<P>,
) -> InlineAccountFollowupPlan<C, A, P> {
    // Open the picker only when the operator has a real choice. A list of 0
    // or 1 means the caller collapsed out the agent's native auth (or never
    // passed any) — dispatch directly instead of presenting a one-item modal.
    if providers.len() >= 2 {
        InlineAccountFollowupPlan::OpenAccountPicker(AccountPickerState::new(
            context, agent, providers,
        ))
    } else {
        InlineAccountFollowupPlan::StartSession {
            context,
            agent,
            account: providers.into_iter().next(),
        }
    }
}

#[must_use]
pub fn inline_picker_shell_plan(key: KeyEvent, _exit_on_q: bool) -> InlinePickerShellPlan {
    use crate::tui::keymap::{
        INLINE_PICKER_SHELL_KEYMAP, InlinePickerShellAction, bridged_keymap_action,
    };
    let event = termrock::input::KeyEvent::from(key);
    match bridged_keymap_action(&INLINE_PICKER_SHELL_KEYMAP, event) {
        Some(InlinePickerShellAction::ScrollLeft) => InlinePickerShellPlan::ScrollHorizontal(-8),
        Some(InlinePickerShellAction::ScrollRight) => InlinePickerShellPlan::ScrollHorizontal(8),
        None => InlinePickerShellPlan::Delegate,
    }
}

#[must_use]
pub fn inline_picker_plan<T>(outcome: jackin_oppicker::ModalOutcome<T>) -> InlinePickerPlan<T> {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(value) => InlinePickerPlan::Commit(value),
        jackin_oppicker::ModalOutcome::Cancel => InlinePickerPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => InlinePickerPlan::Continue,
    }
}

#[must_use]
pub fn op_picker_inline_plan<T>(outcome: jackin_oppicker::ModalOutcome<T>) -> InlinePickerPlan<T> {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(value) => InlinePickerPlan::Commit(value),
        jackin_oppicker::ModalOutcome::Cancel => InlinePickerPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => InlinePickerPlan::Continue,
    }
}

#[must_use]
pub fn file_browser_modal_plan<T>(
    outcome: crate::tui::components::file_browser::FileBrowserOutcome<T>,
) -> FileBrowserModalPlan<T> {
    match outcome {
        crate::tui::components::file_browser::FileBrowserOutcome::Cancel => {
            FileBrowserModalPlan::Dismiss
        }
        crate::tui::components::file_browser::FileBrowserOutcome::ResolveGitUrl(path) => {
            FileBrowserModalPlan::ResolveGitUrl(path)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::OpenGitUrl(url) => {
            FileBrowserModalPlan::OpenUrl(url)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Continue => {
            FileBrowserModalPlan::Continue
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Commit(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
        | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
            FileBrowserModalPlan::ApplyFileBrowserOutcome(outcome)
        }
    }
}

#[must_use]
pub fn auth_source_folder_picker_plan<T>(
    outcome: crate::tui::components::file_browser::FileBrowserOutcome<T>,
) -> AuthSourceFolderPickerPlan<T> {
    match outcome {
        crate::tui::components::file_browser::FileBrowserOutcome::Commit(path) => {
            AuthSourceFolderPickerPlan::Commit(path)
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Cancel => {
            AuthSourceFolderPickerPlan::Close
        }
        crate::tui::components::file_browser::FileBrowserOutcome::Continue
        | crate::tui::components::file_browser::FileBrowserOutcome::OpenGitUrl(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::ResolveGitUrl(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateTo(_)
        | crate::tui::components::file_browser::FileBrowserOutcome::NavigateUp
        | crate::tui::components::file_browser::FileBrowserOutcome::RequestCommit(_) => {
            AuthSourceFolderPickerPlan::KeepModal
        }
    }
}

#[must_use]
pub const fn mount_dst_choice_plan(
    outcome: jackin_oppicker::ModalOutcome<
        crate::tui::components::mount_dst_choice::MountDstChoice,
    >,
) -> MountDstChoicePlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::SamePath,
        ) => MountDstChoicePlan::CommitSamePath,
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::mount_dst_choice::MountDstChoice::Edit,
        ) => MountDstChoicePlan::OpenEditInput,
        jackin_oppicker::ModalOutcome::Cancel => MountDstChoicePlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => MountDstChoicePlan::Continue,
    }
}
