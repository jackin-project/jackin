// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Confirm and picker plans.

use super::{
    BoolConfirmModalPlan, ConfirmSaveModalPlan, CreateOpPickerPlan, DismissibleModalPlan,
    ListGithubPickerPlan, ListRolePickerPlan, SaveDiscardModalPlan, ScopePickerPlan,
    SourcePickerPlan,
};

#[must_use]
pub const fn save_discard_modal_plan(
    outcome: jackin_oppicker::ModalOutcome<crate::tui::components::SaveDiscardChoice>,
) -> SaveDiscardModalPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(crate::tui::components::SaveDiscardChoice::Save) => {
            SaveDiscardModalPlan::Save
        }
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::SaveDiscardChoice::Discard,
        ) => SaveDiscardModalPlan::Discard,
        jackin_oppicker::ModalOutcome::Cancel => SaveDiscardModalPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => SaveDiscardModalPlan::Continue,
    }
}

#[must_use]
pub const fn confirm_save_modal_plan(
    outcome: jackin_oppicker::ModalOutcome<crate::tui::components::confirm_save::SaveChoice>,
) -> ConfirmSaveModalPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::confirm_save::SaveChoice::Save,
        ) => ConfirmSaveModalPlan::Commit,
        jackin_oppicker::ModalOutcome::Cancel => ConfirmSaveModalPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => ConfirmSaveModalPlan::Continue,
    }
}

#[must_use]
pub const fn bool_confirm_modal_plan(
    outcome: jackin_oppicker::ModalOutcome<bool>,
) -> BoolConfirmModalPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(true) => BoolConfirmModalPlan::Confirm,
        jackin_oppicker::ModalOutcome::Commit(false) | jackin_oppicker::ModalOutcome::Cancel => {
            BoolConfirmModalPlan::Dismiss
        }
        jackin_oppicker::ModalOutcome::Continue => BoolConfirmModalPlan::Continue,
    }
}

#[must_use]
pub fn create_op_picker_plan<Reference, Account, Vault, Item, FieldTarget>(
    outcome: jackin_oppicker::ModalOutcome<
        crate::tui::components::op_picker::OpPickerSelection<
            Reference,
            Account,
            Vault,
            Item,
            FieldTarget,
        >,
    >,
) -> CreateOpPickerPlan<
    crate::tui::components::op_picker::OpPickerSelection<
        Reference,
        Account,
        Vault,
        Item,
        FieldTarget,
    >,
> {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(selection) => match selection {
            crate::tui::components::op_picker::OpPickerSelection::NewItem { .. }
            | crate::tui::components::op_picker::OpPickerSelection::EditItemField { .. } => {
                CreateOpPickerPlan::Commit(selection)
            }
            crate::tui::components::op_picker::OpPickerSelection::Existing(_) => {
                CreateOpPickerPlan::Dismiss
            }
        },
        jackin_oppicker::ModalOutcome::Cancel => CreateOpPickerPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => CreateOpPickerPlan::Continue,
    }
}

#[must_use]
pub const fn scope_picker_plan(
    outcome: jackin_oppicker::ModalOutcome<crate::tui::components::scope_picker::ScopeChoice>,
) -> ScopePickerPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::scope_picker::ScopeChoice::AllAgents,
        ) => ScopePickerPlan::AllAgents,
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::scope_picker::ScopeChoice::SpecificAgent,
        ) => ScopePickerPlan::SpecificAgent,
        jackin_oppicker::ModalOutcome::Cancel => ScopePickerPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => ScopePickerPlan::Continue,
    }
}

#[must_use]
pub const fn source_picker_plan(
    outcome: jackin_oppicker::ModalOutcome<crate::tui::components::source_picker::SourceChoice>,
) -> SourcePickerPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::source_picker::SourceChoice::Plain,
        ) => SourcePickerPlan::Plain,
        jackin_oppicker::ModalOutcome::Commit(
            crate::tui::components::source_picker::SourceChoice::Op,
        ) => SourcePickerPlan::Op,
        jackin_oppicker::ModalOutcome::Cancel => SourcePickerPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => SourcePickerPlan::Continue,
    }
}

#[must_use]
pub fn list_github_picker_plan(
    outcome: jackin_oppicker::ModalOutcome<String>,
) -> ListGithubPickerPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(url) => ListGithubPickerPlan::OpenUrl(url),
        jackin_oppicker::ModalOutcome::Cancel => ListGithubPickerPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => ListGithubPickerPlan::Continue,
    }
}

#[must_use]
pub fn list_role_picker_plan<R>(
    outcome: jackin_oppicker::ModalOutcome<R>,
) -> ListRolePickerPlan<R> {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(role) => ListRolePickerPlan::Launch(role),
        jackin_oppicker::ModalOutcome::Cancel => ListRolePickerPlan::Dismiss,
        jackin_oppicker::ModalOutcome::Continue => ListRolePickerPlan::Continue,
    }
}

#[must_use]
pub fn dismissible_modal_plan<T>(
    outcome: jackin_oppicker::ModalOutcome<T>,
) -> DismissibleModalPlan {
    match outcome {
        jackin_oppicker::ModalOutcome::Commit(_) | jackin_oppicker::ModalOutcome::Cancel => {
            DismissibleModalPlan::Dismiss
        }
        jackin_oppicker::ModalOutcome::Continue => DismissibleModalPlan::Continue,
    }
}
