// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings confirm plans.

use super::settings_global_mounts_selected_index;

use super::super::model::GlobalMountConfirm;

use jackin_oppicker::ModalOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsConfirmPlan {
    Continue,
    Commit,
    Cancel { abort_sensitive: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsConfirmCommitPlan {
    Remove {
        remove_index: usize,
        selected: usize,
    },
    Save,
    OpenSavePreview,
    DiscardAll,
    Noop,
}

#[must_use]
pub const fn settings_confirm_plan(
    action: GlobalMountConfirm,
    outcome: ModalOutcome<bool>,
) -> SettingsConfirmPlan {
    match outcome {
        ModalOutcome::Commit(true) => SettingsConfirmPlan::Commit,
        ModalOutcome::Commit(false) | ModalOutcome::Cancel => SettingsConfirmPlan::Cancel {
            abort_sensitive: matches!(action, GlobalMountConfirm::Sensitive),
        },
        ModalOutcome::Continue => SettingsConfirmPlan::Continue,
    }
}

#[must_use]
pub fn settings_confirm_commit_plan(
    action: GlobalMountConfirm,
    selected: usize,
    mount_count: usize,
) -> SettingsConfirmCommitPlan {
    match action {
        GlobalMountConfirm::Remove if selected < mount_count => SettingsConfirmCommitPlan::Remove {
            remove_index: selected,
            selected: settings_global_mounts_selected_index(selected, mount_count - 1),
        },
        GlobalMountConfirm::Remove => SettingsConfirmCommitPlan::Noop,
        GlobalMountConfirm::Save => SettingsConfirmCommitPlan::Save,
        GlobalMountConfirm::Sensitive => SettingsConfirmCommitPlan::OpenSavePreview,
        GlobalMountConfirm::Discard => SettingsConfirmCommitPlan::DiscardAll,
    }
}
