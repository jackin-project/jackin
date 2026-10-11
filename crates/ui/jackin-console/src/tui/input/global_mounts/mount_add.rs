// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Global-mount add flow.

use super::{SettingsModalOutcome, scope_picker_modal, text_modal, text_modal_for_target};

use crate::tui::screens::settings::update as settings_update;
use crate::tui::screens::settings::update::{
    GlobalMountAddFinalizeApplyPlan, GlobalMountAddTextApplyPlan, GlobalMountTextCommitPlan,
};
use crate::tui::screens::settings::view::{
    global_mount_add_draft_lost_message, global_mount_destination_empty_message,
    global_mount_selected_edit_text_plan,
};

use crate::tui::state::{GlobalMountTextTarget, ManagerStage, ManagerState};

pub(crate) fn apply_global_mount_add_text(
    global: &mut crate::tui::state::GlobalMountsState<'_>,
    plan: GlobalMountTextCommitPlan,
) -> SettingsModalOutcome {
    match settings_update::global_mount_add_text_apply_plan(&mut global.add_draft, plan) {
        GlobalMountAddTextApplyPlan::MissingDraft => {
            global.set_error(global_mount_add_draft_lost_message());
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::OpenFileBrowser => {
            SettingsModalOutcome::OpenGlobalMountFileBrowser
        }
        GlobalMountAddTextApplyPlan::OpenAddSource => {
            global.open_sub_modal(text_modal_for_target(GlobalMountTextTarget::AddSource, ""));
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::OpenAddDestination => {
            global.open_sub_modal(text_modal_for_target(
                GlobalMountTextTarget::AddDestination,
                "",
            ));
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::Finalize => {
            finalize_global_mount_add(global);
            SettingsModalOutcome::Continue
        }
        GlobalMountAddTextApplyPlan::Noop => SettingsModalOutcome::Continue,
    }
}

pub(crate) fn open_global_mount_scope_picker(
    global: &mut crate::tui::state::GlobalMountsState<'_>,
) {
    global.start_add_draft();
    global.modals.open(scope_picker_modal());
}

pub(crate) fn finalize_global_mount_add(global: &mut crate::tui::state::GlobalMountsState<'_>) {
    match settings_update::global_mount_add_finalize_apply_plan(
        &global.pending,
        &mut global.add_draft,
    ) {
        GlobalMountAddFinalizeApplyPlan::MissingDraft => {
            global.set_error(global_mount_add_draft_lost_message());
        }
        GlobalMountAddFinalizeApplyPlan::EmptyDestination => {
            global.set_error(global_mount_destination_empty_message());
        }
        GlobalMountAddFinalizeApplyPlan::Add { row, selected } => {
            global.add_row_and_close(row, selected);
        }
    }
}

pub(crate) fn open_edit_text(state: &mut ManagerState<'_>, target: GlobalMountTextTarget) {
    let ManagerStage::Settings(settings) = &mut state.stage else {
        return;
    };
    let global = &mut settings.mounts;
    let Some(plan) = global_mount_selected_edit_text_plan(&global.pending, global.selected, target)
    else {
        return;
    };
    global
        .modals
        .open(text_modal(plan.target, plan.label, &plan.initial));
}
