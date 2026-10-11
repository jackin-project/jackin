// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Global mount commit plans.

use super::{
    GlobalMountAddFinalizeApplyPlan, GlobalMountAddFinalizePlan, GlobalMountAddTextApplyPlan,
    GlobalMountEditTextApplyPlan, GlobalMountGithubOpenPlan, GlobalMountRolePickerCommitPlan,
    GlobalMountScopePickerCommitPlan, GlobalMountTextCommitPlan, RolePickerOpenPlan,
    SettingsGlobalMountsKeyPlan, role_picker_open_plan, settings_global_mounts_add_row_selected,
    settings_global_mounts_added_index,
};

use super::super::model::{GlobalMountDraft, GlobalMountTextTarget, SettingsTrustRow};
use crate::tui::components::scope_picker::ScopeChoice;
use crossterm::event::KeyCode;
use jackin_core::RoleSelector;

#[must_use]
pub const fn settings_global_mounts_key_plan(
    key: KeyCode,
    is_dirty: bool,
    has_sensitive_mount: bool,
    selected: usize,
    mount_count: usize,
) -> SettingsGlobalMountsKeyPlan {
    match key {
        KeyCode::Char('s' | 'S') if has_sensitive_mount => {
            SettingsGlobalMountsKeyPlan::ConfirmSensitiveSave
        }
        KeyCode::Char('s' | 'S') => SettingsGlobalMountsKeyPlan::OpenSavePreview,
        KeyCode::Char('h' | 'H') => SettingsGlobalMountsKeyPlan::ScrollHorizontal { delta: -8 },
        KeyCode::Char('l' | 'L') => SettingsGlobalMountsKeyPlan::ScrollHorizontal { delta: 8 },
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            SettingsGlobalMountsKeyPlan::MoveSelection { delta: -1 }
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            SettingsGlobalMountsKeyPlan::MoveSelection { delta: 1 }
        }
        KeyCode::Char('r' | 'R') => SettingsGlobalMountsKeyPlan::ToggleReadonly,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => {
            use crate::tui::screens::edit_save::{EditSaveDisposition, plan_leave_when_dirty};
            match plan_leave_when_dirty(is_dirty) {
                EditSaveDisposition::ConfirmDiscard => SettingsGlobalMountsKeyPlan::ConfirmDiscard,
                EditSaveDisposition::Noop | EditSaveDisposition::SaveNow => {
                    SettingsGlobalMountsKeyPlan::ReturnToList
                }
            }
        }
        KeyCode::Enter if settings_global_mounts_add_row_selected(selected, mount_count) => {
            SettingsGlobalMountsKeyPlan::OpenAdd
        }
        KeyCode::Char('a' | 'A') => SettingsGlobalMountsKeyPlan::OpenAdd,
        KeyCode::Char('d' | 'D') if mount_count > 0 => SettingsGlobalMountsKeyPlan::ConfirmRemove,
        KeyCode::Char('o' | 'O') => SettingsGlobalMountsKeyPlan::OpenGithub,
        KeyCode::Char('n' | 'N') => {
            SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Rename)
        }
        KeyCode::Char('1') => SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Source),
        KeyCode::Char('2') => {
            SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Destination)
        }
        KeyCode::Char('3') => SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Scope),
        _ => SettingsGlobalMountsKeyPlan::Noop,
    }
}

#[must_use]
pub fn global_mount_text_commit_plan(
    target: &GlobalMountTextTarget,
    value: &str,
) -> GlobalMountTextCommitPlan {
    let trimmed = value.trim();
    match target {
        GlobalMountTextTarget::AddScope => GlobalMountTextCommitPlan::AddScope(
            crate::services::workspace::global_mount_scope_value(trimmed),
        ),
        GlobalMountTextTarget::AddName if trimmed.is_empty() => {
            GlobalMountTextCommitPlan::EmptyName
        }
        GlobalMountTextTarget::AddName => GlobalMountTextCommitPlan::AddName(trimmed.to_owned()),
        GlobalMountTextTarget::AddSource => {
            GlobalMountTextCommitPlan::AddSource(jackin_config::resolve_path(trimmed))
        }
        GlobalMountTextTarget::AddDestination => {
            GlobalMountTextCommitPlan::AddDestination(trimmed.to_owned())
        }
        GlobalMountTextTarget::Source => {
            GlobalMountTextCommitPlan::SetSource(jackin_config::resolve_path(trimmed))
        }
        GlobalMountTextTarget::Destination => {
            GlobalMountTextCommitPlan::SetDestination(trimmed.to_owned())
        }
        GlobalMountTextTarget::Scope => GlobalMountTextCommitPlan::SetScope(
            crate::services::workspace::global_mount_scope_value(trimmed),
        ),
        GlobalMountTextTarget::Rename if trimmed.is_empty() => GlobalMountTextCommitPlan::EmptyName,
        GlobalMountTextTarget::Rename => GlobalMountTextCommitPlan::Rename(trimmed.to_owned()),
    }
}

#[must_use]
pub fn global_mount_add_finalize_plan(
    pending: &[jackin_config::GlobalMountRow],
    mut draft: GlobalMountDraft,
) -> GlobalMountAddFinalizePlan {
    if draft.dst.trim().is_empty() {
        return GlobalMountAddFinalizePlan::EmptyDestination(draft);
    }
    draft.name = crate::services::workspace::unique_global_mount_name(
        pending,
        draft.scope.as_deref(),
        &draft.dst,
    );
    let selected = settings_global_mounts_added_index(pending.len() + 1);
    GlobalMountAddFinalizePlan::Add {
        row: jackin_config::GlobalMountRow {
            scope: draft.scope,
            name: draft.name,
            mount: crate::services::workspace::shared_mount_config(draft.src, draft.dst, false),
        },
        selected,
    }
}

pub fn global_mount_add_finalize_apply_plan(
    pending: &[jackin_config::GlobalMountRow],
    draft: &mut Option<GlobalMountDraft>,
) -> GlobalMountAddFinalizeApplyPlan {
    let Some(taken) = draft.take() else {
        return GlobalMountAddFinalizeApplyPlan::MissingDraft;
    };
    match global_mount_add_finalize_plan(pending, taken) {
        GlobalMountAddFinalizePlan::EmptyDestination(taken) => {
            *draft = Some(taken);
            GlobalMountAddFinalizeApplyPlan::EmptyDestination
        }
        GlobalMountAddFinalizePlan::Add { row, selected } => {
            GlobalMountAddFinalizeApplyPlan::Add { row, selected }
        }
    }
}

pub fn set_global_mount_add_draft_destination(
    draft: &mut Option<GlobalMountDraft>,
    dst: impl Into<String>,
) -> bool {
    let Some(draft) = draft.as_mut() else {
        return false;
    };
    draft.dst = dst.into();
    true
}

pub fn global_mount_add_text_apply_plan(
    draft: &mut Option<GlobalMountDraft>,
    plan: GlobalMountTextCommitPlan,
) -> GlobalMountAddTextApplyPlan {
    match plan {
        GlobalMountTextCommitPlan::AddScope(scope) => {
            let Some(draft) = draft.as_mut() else {
                return GlobalMountAddTextApplyPlan::MissingDraft;
            };
            draft.scope = scope;
            GlobalMountAddTextApplyPlan::OpenFileBrowser
        }
        GlobalMountTextCommitPlan::AddName(name) => {
            let Some(draft) = draft.as_mut() else {
                return GlobalMountAddTextApplyPlan::MissingDraft;
            };
            draft.name = name;
            GlobalMountAddTextApplyPlan::OpenAddSource
        }
        GlobalMountTextCommitPlan::AddSource(src) => {
            let Some(draft) = draft.as_mut() else {
                return GlobalMountAddTextApplyPlan::MissingDraft;
            };
            draft.src = src;
            GlobalMountAddTextApplyPlan::OpenAddDestination
        }
        GlobalMountTextCommitPlan::AddDestination(dst) => {
            let Some(draft) = draft.as_mut() else {
                return GlobalMountAddTextApplyPlan::MissingDraft;
            };
            draft.dst = dst;
            GlobalMountAddTextApplyPlan::Finalize
        }
        _ => GlobalMountAddTextApplyPlan::Noop,
    }
}

pub fn global_mount_edit_text_apply_plan(
    rows: &mut [jackin_config::GlobalMountRow],
    selected: usize,
    plan: GlobalMountTextCommitPlan,
) -> GlobalMountEditTextApplyPlan {
    match plan {
        GlobalMountTextCommitPlan::SetSource(value) => {
            let Some(row) = rows.get_mut(selected) else {
                return GlobalMountEditTextApplyPlan::MissingRow;
            };
            row.mount.src = value;
            GlobalMountEditTextApplyPlan::Applied
        }
        GlobalMountTextCommitPlan::SetDestination(value) => {
            let Some(row) = rows.get_mut(selected) else {
                return GlobalMountEditTextApplyPlan::MissingRow;
            };
            row.mount.dst = value;
            GlobalMountEditTextApplyPlan::Applied
        }
        GlobalMountTextCommitPlan::SetScope(scope) => {
            let Some(row) = rows.get_mut(selected) else {
                return GlobalMountEditTextApplyPlan::MissingRow;
            };
            row.scope = scope;
            GlobalMountEditTextApplyPlan::Applied
        }
        GlobalMountTextCommitPlan::Rename(value) => {
            let Some(row) = rows.get_mut(selected) else {
                return GlobalMountEditTextApplyPlan::MissingRow;
            };
            row.name = value;
            GlobalMountEditTextApplyPlan::Applied
        }
        GlobalMountTextCommitPlan::EmptyName => GlobalMountEditTextApplyPlan::EmptyName,
        GlobalMountTextCommitPlan::AddScope(_)
        | GlobalMountTextCommitPlan::AddName(_)
        | GlobalMountTextCommitPlan::AddSource(_)
        | GlobalMountTextCommitPlan::AddDestination(_) => GlobalMountEditTextApplyPlan::Noop,
    }
}

#[must_use]
pub const fn global_mount_scope_picker_commit_plan(
    choice: ScopeChoice,
) -> GlobalMountScopePickerCommitPlan {
    match choice {
        ScopeChoice::AllAgents => GlobalMountScopePickerCommitPlan::ApplyAllAgentsScope,
        ScopeChoice::SpecificAgent => GlobalMountScopePickerCommitPlan::OpenRolePicker,
    }
}

#[must_use]
pub fn global_mount_role_picker_roles(rows: &[SettingsTrustRow]) -> Vec<RoleSelector> {
    rows.iter()
        .filter_map(|row| RoleSelector::parse(&row.role).ok())
        .collect()
}

#[must_use]
pub fn global_mount_role_picker_open_plan(rows: &[SettingsTrustRow]) -> RolePickerOpenPlan {
    role_picker_open_plan(global_mount_role_picker_roles(rows))
}

pub fn global_mount_role_picker_commit_plan(
    draft: &mut Option<GlobalMountDraft>,
    role: &RoleSelector,
) -> GlobalMountRolePickerCommitPlan {
    let Some(draft) = draft.as_mut() else {
        return GlobalMountRolePickerCommitPlan::MissingDraft;
    };
    draft.scope = Some(role.key());
    GlobalMountRolePickerCommitPlan::OpenFileBrowser
}

#[must_use]
pub fn global_mount_github_open_plan(
    rows: &[jackin_config::GlobalMountRow],
    selected: usize,
    cache: &crate::mount_info_cache::MountInfoCache,
) -> GlobalMountGithubOpenPlan {
    let Some(row) = rows.get(selected) else {
        return GlobalMountGithubOpenPlan::NoSelection;
    };
    match cache.github_web_url(&row.mount.src) {
        Some(web_url) => GlobalMountGithubOpenPlan::Open(web_url),
        None => GlobalMountGithubOpenPlan::NoGithubUrl,
    }
}
