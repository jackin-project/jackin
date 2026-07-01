//! Settings screen update logic: handle keyboard events and produce effects
//! for the General, Mounts, Environments, Auth, and Trust tab group.
//!
//! Not responsible for: rendering (see `view`) or state definitions (see
//! `model`).

use std::collections::{BTreeMap, BTreeSet};

use super::model::{
    GlobalMountConfirm, GlobalMountDraft, GlobalMountTextTarget, SettingsEnvConfig,
    SettingsEnvEnterPlan, SettingsEnvRow, SettingsEnvScope, SettingsEnvTextTarget,
    SettingsHoverTarget, SettingsTab, SettingsTrustRow, SettingsTrustState,
};
use crate::tui::auth::{AuthKind, AuthMode, auth_mode_requires_credential};
use crate::tui::components::scope_picker::ScopeChoice;
use crossterm::event::KeyCode;
use jackin_core::{EnvValue, RoleSelector};
use jackin_tui::ModalOutcome;
use ratatui::layout::Rect;

#[must_use]
pub const fn previous_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Trust,
        SettingsTab::Mounts => SettingsTab::General,
        SettingsTab::Environments => SettingsTab::Mounts,
        SettingsTab::Auth => SettingsTab::Environments,
        SettingsTab::Trust => SettingsTab::Auth,
    }
}

#[must_use]
pub const fn next_settings_tab(tab: SettingsTab) -> SettingsTab {
    match tab {
        SettingsTab::General => SettingsTab::Mounts,
        SettingsTab::Mounts => SettingsTab::Environments,
        SettingsTab::Environments => SettingsTab::Auth,
        SettingsTab::Auth => SettingsTab::Trust,
        SettingsTab::Trust => SettingsTab::General,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsTabMovePlan {
    pub active_tab: SettingsTab,
    pub tab_bar_focused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsShellKeyPlan {
    MoveTab { delta: isize, focus_tab_bar: bool },
    FocusContent,
    FocusTabBar { clear_auth_kind: bool },
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsTopLevelKeyPlan {
    MoveTab { delta: isize, focus_tab_bar: bool },
    FocusContent,
    FocusTabBar { clear_auth_kind: bool },
    SetEnvRoleExpanded { role: String, expanded: bool },
    Consume,
    Delegate(SettingsTab),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsGeneralKeyPlan {
    MoveSelection { delta: isize },
    ToggleSelected,
    ConfirmDiscard,
    ReturnToList,
    Save,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvKeyPlan {
    MoveSelection { delta: isize },
    ConfirmDiscard,
    ReturnToList,
    OpenAdd,
    Save,
    ConfirmDelete,
    ToggleMask,
    OpenPicker,
    OpenEnterModal,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvHeaderKeyPlan {
    SetExpanded { role: String, expanded: bool },
    Consume,
    Continue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTrustKeyPlan {
    MoveSelection { delta: isize },
    ScrollHorizontal { delta: i16 },
    ToggleSelected,
    ConfirmDiscard,
    ReturnToList,
    Save,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAuthKeyPlan {
    ClearKind,
    MoveSelection { delta: isize },
    EnterKind,
    ConfirmDiscard,
    ReturnToList,
    OpenForm,
    Save,
    Noop,
}

#[must_use]
pub const fn settings_tab_move_plan(
    active_tab: SettingsTab,
    delta: isize,
    focus_tab_bar: bool,
) -> SettingsTabMovePlan {
    SettingsTabMovePlan {
        active_tab: if delta.is_negative() {
            previous_settings_tab(active_tab)
        } else {
            next_settings_tab(active_tab)
        },
        tab_bar_focused: focus_tab_bar,
    }
}

#[must_use]
pub const fn settings_tab_select_plan(selected_tab: SettingsTab) -> SettingsTabMovePlan {
    SettingsTabMovePlan {
        active_tab: selected_tab,
        tab_bar_focused: true,
    }
}

#[must_use]
pub const fn settings_tab_bar_focus_plan(focused: bool) -> bool {
    focused
}

#[must_use]
pub fn settings_tab_hover_plan(row: u16, col: u16) -> Option<usize> {
    let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    crate::tui::layout::tab_hover_index_at_position(row, col, &labels)
}

#[must_use]
pub fn settings_tab_hover_target_plan(
    mounts_modal_open: bool,
    env_modal_open: bool,
    row: u16,
    col: u16,
) -> Option<SettingsHoverTarget> {
    (!mounts_modal_open && !env_modal_open)
        .then(|| settings_tab_hover_plan(row, col).map(SettingsHoverTarget::Tab))
        .flatten()
}

#[must_use]
pub const fn settings_shell_key_plan(
    key: KeyCode,
    tab_bar_focused: bool,
    auth_kind_selected: bool,
) -> SettingsShellKeyPlan {
    if tab_bar_focused {
        match key {
            KeyCode::Left | KeyCode::BackTab => {
                return SettingsShellKeyPlan::MoveTab {
                    delta: -1,
                    focus_tab_bar: true,
                };
            }
            KeyCode::Right => {
                return SettingsShellKeyPlan::MoveTab {
                    delta: 1,
                    focus_tab_bar: true,
                };
            }
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j' | 'J') => {
                return SettingsShellKeyPlan::FocusContent;
            }
            _ => {}
        }
    }

    match key {
        KeyCode::Tab => SettingsShellKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        },
        KeyCode::BackTab => SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: false,
        },
        KeyCode::Esc if !tab_bar_focused => SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: auth_kind_selected,
        },
        _ => SettingsShellKeyPlan::Continue,
    }
}

#[must_use]
pub const fn settings_general_key_plan(key: KeyCode, is_dirty: bool) -> SettingsGeneralKeyPlan {
    match key {
        KeyCode::Up | KeyCode::Char('k' | 'K') => {
            SettingsGeneralKeyPlan::MoveSelection { delta: -1 }
        }
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            SettingsGeneralKeyPlan::MoveSelection { delta: 1 }
        }
        KeyCode::Char(' ') => SettingsGeneralKeyPlan::ToggleSelected,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if is_dirty => {
            SettingsGeneralKeyPlan::ConfirmDiscard
        }
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => SettingsGeneralKeyPlan::ReturnToList,
        KeyCode::Char('s' | 'S') => SettingsGeneralKeyPlan::Save,
        _ => SettingsGeneralKeyPlan::Noop,
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "tracked in codebase-health-enforcement"
)]
#[must_use]
pub const fn settings_env_key_plan(
    key: KeyCode,
    plain_modifier: bool,
    is_dirty: bool,
    op_available: bool,
    selected_is_op_ref: bool,
) -> SettingsEnvKeyPlan {
    match key {
        KeyCode::Up | KeyCode::Char('k' | 'K') => SettingsEnvKeyPlan::MoveSelection { delta: -1 },
        KeyCode::Down | KeyCode::Char('j' | 'J') => SettingsEnvKeyPlan::MoveSelection { delta: 1 },
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if is_dirty => SettingsEnvKeyPlan::ConfirmDiscard,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => SettingsEnvKeyPlan::ReturnToList,
        KeyCode::Char('a' | 'A') => SettingsEnvKeyPlan::OpenAdd,
        KeyCode::Char('s' | 'S') => SettingsEnvKeyPlan::Save,
        KeyCode::Char('d' | 'D') if plain_modifier => SettingsEnvKeyPlan::ConfirmDelete,
        KeyCode::Char('m' | 'M') if plain_modifier => SettingsEnvKeyPlan::ToggleMask,
        KeyCode::Char('p' | 'P') if plain_modifier && op_available => {
            SettingsEnvKeyPlan::OpenPicker
        }
        KeyCode::Enter if selected_is_op_ref && op_available => SettingsEnvKeyPlan::OpenPicker,
        KeyCode::Enter => SettingsEnvKeyPlan::OpenEnterModal,
        _ => SettingsEnvKeyPlan::Noop,
    }
}

#[must_use]
pub const fn settings_auth_key_plan(
    key: KeyCode,
    is_dirty: bool,
    has_selected_kind: bool,
    selected_detail_row_is_focusable: bool,
) -> SettingsAuthKeyPlan {
    match key {
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if has_selected_kind => {
            SettingsAuthKeyPlan::ClearKind
        }
        KeyCode::Up | KeyCode::Char('k' | 'K') => SettingsAuthKeyPlan::MoveSelection { delta: -1 },
        KeyCode::Down | KeyCode::Char('j' | 'J') => SettingsAuthKeyPlan::MoveSelection { delta: 1 },
        KeyCode::Enter if !has_selected_kind => SettingsAuthKeyPlan::EnterKind,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if is_dirty => SettingsAuthKeyPlan::ConfirmDiscard,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => SettingsAuthKeyPlan::ReturnToList,
        KeyCode::Enter if selected_detail_row_is_focusable => SettingsAuthKeyPlan::OpenForm,
        KeyCode::Char('s' | 'S') => SettingsAuthKeyPlan::Save,
        _ => SettingsAuthKeyPlan::Noop,
    }
}

#[must_use]
pub fn settings_env_header_key_plan(
    key: KeyCode,
    active_tab: SettingsTab,
    selected_row: Option<&SettingsEnvRow>,
) -> SettingsEnvHeaderKeyPlan {
    if active_tab != SettingsTab::Environments {
        return SettingsEnvHeaderKeyPlan::Continue;
    }

    match key {
        KeyCode::Right => match selected_row {
            Some(SettingsEnvRow::RoleHeader {
                role,
                expanded: false,
            }) => SettingsEnvHeaderKeyPlan::SetExpanded {
                role: role.clone(),
                expanded: true,
            },
            _ => SettingsEnvHeaderKeyPlan::Consume,
        },
        KeyCode::Left => match selected_row {
            Some(SettingsEnvRow::RoleHeader {
                role,
                expanded: true,
            }) => SettingsEnvHeaderKeyPlan::SetExpanded {
                role: role.clone(),
                expanded: false,
            },
            _ => SettingsEnvHeaderKeyPlan::Consume,
        },
        _ => SettingsEnvHeaderKeyPlan::Continue,
    }
}

#[must_use]
pub fn settings_env_selected_header_key_plan<V>(
    key: KeyCode,
    active_tab: SettingsTab,
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> SettingsEnvHeaderKeyPlan {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_header_key_plan(key, active_tab, rows.get(selected))
}

#[must_use]
pub fn settings_top_level_key_plan<V>(
    key: KeyCode,
    active_tab: SettingsTab,
    tab_bar_focused: bool,
    auth_kind_selected: bool,
    env_pending: &SettingsEnvConfig<V>,
    env_expanded_roles: &BTreeSet<String>,
    env_selected: usize,
) -> SettingsTopLevelKeyPlan {
    match settings_shell_key_plan(key, tab_bar_focused, auth_kind_selected) {
        SettingsShellKeyPlan::MoveTab {
            delta,
            focus_tab_bar,
        } => {
            return SettingsTopLevelKeyPlan::MoveTab {
                delta,
                focus_tab_bar,
            };
        }
        SettingsShellKeyPlan::FocusContent => {
            return SettingsTopLevelKeyPlan::FocusContent;
        }
        SettingsShellKeyPlan::FocusTabBar { clear_auth_kind } => {
            return SettingsTopLevelKeyPlan::FocusTabBar { clear_auth_kind };
        }
        SettingsShellKeyPlan::Continue => {}
    }

    match settings_env_selected_header_key_plan(
        key,
        active_tab,
        env_pending,
        env_expanded_roles,
        env_selected,
    ) {
        SettingsEnvHeaderKeyPlan::SetExpanded { role, expanded } => {
            SettingsTopLevelKeyPlan::SetEnvRoleExpanded { role, expanded }
        }
        SettingsEnvHeaderKeyPlan::Consume => SettingsTopLevelKeyPlan::Consume,
        SettingsEnvHeaderKeyPlan::Continue => SettingsTopLevelKeyPlan::Delegate(active_tab),
    }
}

#[must_use]
pub fn settings_env_selected_key_matches<V>(
    config: &SettingsEnvConfig<V>,
    rows: &[SettingsEnvRow],
    selected: usize,
    predicate: impl FnOnce(&V) -> bool,
) -> bool {
    matches!(
        rows.get(selected),
        Some(SettingsEnvRow::Key { scope, key })
            if settings_env_value(config, scope, key).is_some_and(predicate)
    )
}

#[must_use]
pub fn settings_env_selected_key_is_op_ref(
    config: &SettingsEnvConfig<EnvValue>,
    rows: &[SettingsEnvRow],
    selected: usize,
) -> bool {
    settings_env_selected_key_matches(config, rows, selected, |value| {
        matches!(value, EnvValue::OpRef(_))
    })
}

#[must_use]
pub fn settings_env_selected_is_op_ref(
    config: &SettingsEnvConfig<EnvValue>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> bool {
    let rows = settings_env_flat_rows(config, expanded_roles);
    settings_env_selected_key_is_op_ref(config, &rows, selected)
}

#[must_use]
pub fn settings_env_delete_key_for_row(row: Option<&SettingsEnvRow>) -> Option<&str> {
    match row {
        Some(SettingsEnvRow::Key { key, .. }) => Some(key.as_str()),
        _ => None,
    }
}

#[must_use]
pub fn settings_env_selected_delete_key<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> Option<String> {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_delete_key_for_row(rows.get(selected)).map(str::to_owned)
}

#[must_use]
pub const fn settings_trust_key_plan(key: KeyCode, is_dirty: bool) -> SettingsTrustKeyPlan {
    match key {
        KeyCode::Up | KeyCode::Char('k' | 'K') => SettingsTrustKeyPlan::MoveSelection { delta: -1 },
        KeyCode::Down | KeyCode::Char('j' | 'J') => {
            SettingsTrustKeyPlan::MoveSelection { delta: 1 }
        }
        KeyCode::Char('h' | 'H') => SettingsTrustKeyPlan::ScrollHorizontal { delta: -8 },
        KeyCode::Char('l' | 'L') => SettingsTrustKeyPlan::ScrollHorizontal { delta: 8 },
        KeyCode::Char(' ') => SettingsTrustKeyPlan::ToggleSelected,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if is_dirty => SettingsTrustKeyPlan::ConfirmDiscard,
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => SettingsTrustKeyPlan::ReturnToList,
        KeyCode::Char('s' | 'S') => SettingsTrustKeyPlan::Save,
        _ => SettingsTrustKeyPlan::Noop,
    }
}

#[must_use]
pub fn settings_tab_at_position(row: u16, col: u16) -> Option<SettingsTab> {
    let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    let idx = crate::tui::layout::tab_cell_at_position(row, col, &labels)?;
    SettingsTab::ALL.get(idx).copied()
}

#[must_use]
pub fn settings_auth_detail_row_count(kind: AuthKind, mode: AuthMode) -> usize {
    settings_auth_detail_rows(kind, mode).len()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsAuthDetailRow {
    Mode,
    Source,
    SourceFolder,
    Spacer,
}

#[must_use]
pub fn settings_auth_detail_rows(kind: AuthKind, mode: AuthMode) -> Vec<SettingsAuthDetailRow> {
    let mut rows = vec![SettingsAuthDetailRow::Mode];
    if auth_mode_requires_credential(kind, mode) {
        rows.push(SettingsAuthDetailRow::Source);
    }
    if crate::tui::auth::auth_mode_supports_source_folder(kind, mode) {
        rows.push(SettingsAuthDetailRow::SourceFolder);
    }
    rows.push(SettingsAuthDetailRow::Spacer);
    rows
}

#[must_use]
pub const fn settings_auth_row_is_focusable(row: SettingsAuthDetailRow) -> bool {
    matches!(row, SettingsAuthDetailRow::Mode)
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountTextCommitPlan {
    AddScope(Option<String>),
    AddName(String),
    AddSource(String),
    AddDestination(String),
    SetSource(String),
    SetDestination(String),
    SetScope(Option<String>),
    Rename(String),
    EmptyName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountEditTextApplyPlan {
    MissingRow,
    EmptyName,
    Applied,
    Noop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RolePickerOpenPlan {
    NoRoles,
    Open(Vec<RoleSelector>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountGithubOpenPlan {
    NoSelection,
    NoGithubUrl,
    Open(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountAddFinalizePlan {
    EmptyDestination(GlobalMountDraft),
    Add {
        row: jackin_config::GlobalMountRow,
        selected: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlobalMountAddFinalizeApplyPlan {
    MissingDraft,
    EmptyDestination,
    Add {
        row: jackin_config::GlobalMountRow,
        selected: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountRolePickerCommitPlan {
    MissingDraft,
    OpenFileBrowser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountAddTextApplyPlan {
    MissingDraft,
    OpenFileBrowser,
    OpenAddSource,
    OpenAddDestination,
    Finalize,
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalMountScopePickerCommitPlan {
    ApplyAllAgentsScope,
    OpenRolePicker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvTextCommitPlan {
    EmptyKey {
        scope: SettingsEnvScope,
    },
    SetPendingPickerValue {
        scope: SettingsEnvScope,
        key: String,
    },
    OpenSourcePicker {
        scope: SettingsEnvScope,
        key: String,
    },
    SetPlainValue {
        scope: SettingsEnvScope,
        key: String,
        value: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvSourcePickerSelection {
    Plain,
    Op,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvSourcePickerCommitPlan {
    MissingPendingKey,
    OpenPlainText {
        scope: SettingsEnvScope,
        key: String,
    },
    OpenOpPicker {
        scope: SettingsEnvScope,
        key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvOpPickerCommitPlan {
    MissingTarget,
    SetExisting {
        scope: SettingsEnvScope,
        key: String,
    },
    StashForNewKey {
        scope: SettingsEnvScope,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsEnvScopePickerSelection {
    AllAgents,
    SpecificAgent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsEnvScopePickerCommitPlan {
    OpenGlobalKeyInput { scope: SettingsEnvScope },
    OpenRolePicker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsEnvRolePickerCommitPlan {
    pub scope: SettingsEnvScope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsGlobalMountsKeyPlan {
    ConfirmSensitiveSave,
    OpenSavePreview,
    ScrollHorizontal { delta: i16 },
    MoveSelection { delta: isize },
    ToggleReadonly,
    ConfirmDiscard,
    ReturnToList,
    OpenAdd,
    ConfirmRemove,
    OpenGithub,
    OpenEdit(GlobalMountTextTarget),
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
        KeyCode::Esc | KeyCode::Char('q' | 'Q') if is_dirty => {
            SettingsGlobalMountsKeyPlan::ConfirmDiscard
        }
        KeyCode::Esc | KeyCode::Char('q' | 'Q') => SettingsGlobalMountsKeyPlan::ReturnToList,
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

#[must_use]
pub fn settings_env_text_commit_plan(
    target: &SettingsEnvTextTarget,
    value: &str,
    has_pending_picker_value: bool,
) -> SettingsEnvTextCommitPlan {
    match target {
        SettingsEnvTextTarget::EnvKey { scope } => {
            let key = value.trim();
            if key.is_empty() {
                return SettingsEnvTextCommitPlan::EmptyKey {
                    scope: scope.clone(),
                };
            }
            if has_pending_picker_value {
                SettingsEnvTextCommitPlan::SetPendingPickerValue {
                    scope: scope.clone(),
                    key: key.to_owned(),
                }
            } else {
                SettingsEnvTextCommitPlan::OpenSourcePicker {
                    scope: scope.clone(),
                    key: key.to_owned(),
                }
            }
        }
        SettingsEnvTextTarget::EnvValue { scope, key } => {
            SettingsEnvTextCommitPlan::SetPlainValue {
                scope: scope.clone(),
                key: key.clone(),
                value: value.to_owned(),
            }
        }
    }
}

#[must_use]
pub fn settings_env_source_picker_commit_plan(
    selection: SettingsEnvSourcePickerSelection,
    pending_env_key: Option<&(SettingsEnvScope, String)>,
) -> SettingsEnvSourcePickerCommitPlan {
    let Some((scope, key)) = pending_env_key else {
        return SettingsEnvSourcePickerCommitPlan::MissingPendingKey;
    };
    match selection {
        SettingsEnvSourcePickerSelection::Plain => {
            SettingsEnvSourcePickerCommitPlan::OpenPlainText {
                scope: scope.clone(),
                key: key.clone(),
            }
        }
        SettingsEnvSourcePickerSelection::Op => SettingsEnvSourcePickerCommitPlan::OpenOpPicker {
            scope: scope.clone(),
            key: key.clone(),
        },
    }
}

#[must_use]
pub fn settings_env_op_picker_commit_plan(
    pending_picker_target: Option<&(SettingsEnvScope, Option<String>)>,
) -> SettingsEnvOpPickerCommitPlan {
    match pending_picker_target {
        Some((scope, Some(key))) => SettingsEnvOpPickerCommitPlan::SetExisting {
            scope: scope.clone(),
            key: key.clone(),
        },
        Some((scope, None)) => SettingsEnvOpPickerCommitPlan::StashForNewKey {
            scope: scope.clone(),
        },
        None => SettingsEnvOpPickerCommitPlan::MissingTarget,
    }
}

#[must_use]
pub const fn settings_env_scope_picker_commit_plan(
    selection: SettingsEnvScopePickerSelection,
) -> SettingsEnvScopePickerCommitPlan {
    match selection {
        SettingsEnvScopePickerSelection::AllAgents => {
            SettingsEnvScopePickerCommitPlan::OpenGlobalKeyInput {
                scope: SettingsEnvScope::Global,
            }
        }
        SettingsEnvScopePickerSelection::SpecificAgent => {
            SettingsEnvScopePickerCommitPlan::OpenRolePicker
        }
    }
}

#[must_use]
pub fn settings_env_role_picker_commit_plan(
    role: &RoleSelector,
) -> SettingsEnvRolePickerCommitPlan {
    SettingsEnvRolePickerCommitPlan {
        scope: SettingsEnvScope::Role(role.key()),
    }
}

#[must_use]
pub fn settings_env_role_picker_roles<V>(pending: &SettingsEnvConfig<V>) -> Vec<RoleSelector> {
    pending
        .roles
        .keys()
        .filter_map(|role| RoleSelector::parse(role).ok())
        .collect()
}

#[must_use]
pub fn settings_env_role_picker_open_plan<V>(pending: &SettingsEnvConfig<V>) -> RolePickerOpenPlan {
    role_picker_open_plan(settings_env_role_picker_roles(pending))
}

#[must_use]
pub fn role_picker_open_plan(roles: Vec<RoleSelector>) -> RolePickerOpenPlan {
    if roles.is_empty() {
        RolePickerOpenPlan::NoRoles
    } else {
        RolePickerOpenPlan::Open(roles)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsAuthKindPlan<K> {
    pub selected_kind: Option<K>,
    pub selected: usize,
}

#[must_use]
pub const fn clear_settings_auth_kind_plan<K>() -> SettingsAuthKindPlan<K> {
    SettingsAuthKindPlan {
        selected_kind: None,
        selected: 0,
    }
}

#[must_use]
pub fn enter_settings_auth_kind_plan<K>(
    selected_kind: Option<K>,
) -> Option<SettingsAuthKindPlan<K>> {
    match selected_kind {
        Some(kind) => Some(SettingsAuthKindPlan {
            selected_kind: Some(kind),
            selected: 0,
        }),
        None => None,
    }
}

#[must_use]
pub fn settings_auth_selection_plan(
    selected: usize,
    rows: &[SettingsAuthDetailRow],
    delta: isize,
) -> usize {
    if rows.is_empty() {
        return 0;
    }
    let selected = selected.min(rows.len().saturating_sub(1));
    let focusable: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| settings_auth_row_is_focusable(*row).then_some(index))
        .collect();
    if focusable.is_empty() {
        return selected;
    }
    let pos = focusable
        .iter()
        .position(|index| *index == selected)
        .unwrap_or_else(|| {
            focusable
                .iter()
                .position(|index| *index > selected)
                .unwrap_or(focusable.len() - 1)
        });
    let next = crate::tui::focus::moved_selection(pos, focusable.len(), delta);
    focusable[next]
}

#[must_use]
pub fn settings_auth_selected_index(selected: usize, row_count: usize) -> usize {
    selected.min(row_count.saturating_sub(1))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsSelectionScrollPlan {
    pub selected: usize,
    pub scroll_y: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsTrustRowSelectPlan {
    pub selected: Option<usize>,
    pub content_focused: bool,
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "Four orthogonal settings-tab focus flags (mounts, env, auth, trust) \
              describing which sub-scroll pane is currently focusable — each tracks \
              a distinct sub-pane and is consumed individually by the focus router. \
              Mutually exclusive in practice but naming each sub-pane directly is \
              clearer than a single enum variant in plan-shaped code."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsScrollFocusPlan {
    pub mounts: bool,
    pub env: bool,
    pub auth: bool,
    pub trust: bool,
}

#[must_use]
pub fn settings_horizontal_scroll_plan(
    current_scroll_x: u16,
    delta: i16,
    term_width: u16,
    content_width: usize,
) -> u16 {
    crate::tui::update::term_width_scroll_plan(current_scroll_x, delta, term_width, content_width)
}

#[must_use]
pub const fn settings_scroll_focus_plan(
    active_tab: SettingsTab,
    modal_open: bool,
    in_content: bool,
) -> SettingsScrollFocusPlan {
    if modal_open {
        return SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: false,
            trust: false,
        };
    }
    SettingsScrollFocusPlan {
        mounts: matches!(active_tab, SettingsTab::Mounts) && in_content,
        env: matches!(active_tab, SettingsTab::Environments) && in_content,
        auth: matches!(active_tab, SettingsTab::Auth) && in_content,
        trust: matches!(active_tab, SettingsTab::Trust) && in_content,
    }
}

#[expect(
    clippy::fn_params_excessive_bools,
    reason = "tracked in codebase-health-enforcement"
)]
#[must_use]
pub const fn settings_modal_open(
    error_popup_open: bool,
    mounts_modal_open: bool,
    env_modal_open: bool,
    auth_modal_open: bool,
) -> bool {
    error_popup_open || mounts_modal_open || env_modal_open || auth_modal_open
}

#[must_use]
pub const fn settings_trust_row_select_plan(
    selected: usize,
    row_count: usize,
) -> SettingsTrustRowSelectPlan {
    SettingsTrustRowSelectPlan {
        selected: if selected < row_count {
            Some(selected)
        } else {
            None
        },
        content_focused: true,
    }
}

#[must_use]
pub fn settings_trust_selection_plan(
    selected: usize,
    row_count: usize,
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let selected = crate::tui::focus::moved_selection(selected, row_count, delta);
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::tui::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_env_selection_plan(
    selected: usize,
    rows: &[SettingsEnvRow],
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let max = rows.len().saturating_sub(1);
    let candidate = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta as usize).min(max)
    };
    let selected = if delta.is_negative() {
        step_cursor_up_by(candidate, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    } else {
        step_cursor_down_by(candidate, max, |idx| {
            matches!(rows.get(idx), Some(SettingsEnvRow::SectionSpacer))
        })
    };
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::tui::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_global_mounts_selection_plan(
    selected: usize,
    mount_count: usize,
    delta: isize,
    current_scroll_y: u16,
    term_height: u16,
    footer_h: u16,
) -> SettingsSelectionScrollPlan {
    let selected = if delta.is_negative() {
        selected.saturating_sub(delta.unsigned_abs())
    } else {
        selected.saturating_add(delta as usize).min(mount_count)
    };
    SettingsSelectionScrollPlan {
        selected,
        scroll_y: crate::tui::focus::cursor_scroll_for_panel(
            selected,
            current_scroll_y,
            term_height,
            footer_h,
        ),
    }
}

#[must_use]
pub fn settings_global_mounts_selected_index(selected: usize, mount_count: usize) -> usize {
    selected.min(mount_count)
}

#[must_use]
pub const fn settings_global_mounts_add_row_selected(selected: usize, mount_count: usize) -> bool {
    selected == mount_count
}

#[must_use]
pub fn settings_global_mounts_added_index(mount_count: usize) -> usize {
    mount_count.saturating_sub(1)
}

#[must_use]
pub fn settings_trust_row_at_position(
    area: Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    row_count: usize,
) -> Option<usize> {
    if !crate::tui::layout::point_in_rect(col, row, area) {
        return None;
    }
    let line = usize::from(row.saturating_sub(area.y + 1)) + usize::from(scroll_y);
    let row = line.checked_sub(1)?;
    (row < row_count).then_some(row)
}

#[must_use]
pub fn settings_trust_hover_target_at_position(
    active_tab: SettingsTab,
    mounts_modal_open: bool,
    area: Rect,
    col: u16,
    row: u16,
    scroll_y: u16,
    row_count: usize,
) -> Option<SettingsHoverTarget> {
    if active_tab != SettingsTab::Trust || mounts_modal_open {
        return None;
    }
    settings_trust_row_at_position(area, col, row, scroll_y, row_count)
        .map(SettingsHoverTarget::TrustRow)
}

#[must_use]
pub fn settings_trust_clickable_at_position(
    active_tab: SettingsTab,
    modal_open: bool,
    content_area: Rect,
    col: u16,
    row: u16,
) -> bool {
    active_tab == SettingsTab::Trust
        && !modal_open
        && crate::tui::layout::point_in_rect(col, row, content_area)
}

#[must_use]
pub fn trust_content_width(state: &SettingsTrustState) -> usize {
    state
        .pending
        .iter()
        .map(|row| 42 + jackin_tui::display_cols(&row.git))
        .chain(["  Role                         Trust      Git".len()])
        .max()
        .unwrap_or(0)
}

pub fn set_role_expanded(expanded_roles: &mut BTreeSet<String>, role: String, expanded: bool) {
    if expanded {
        expanded_roles.insert(role);
    } else {
        expanded_roles.remove(&role);
    }
}

#[must_use]
pub fn settings_env_flat_rows<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
) -> Vec<SettingsEnvRow> {
    let mut rows = Vec::new();
    for key in pending.env.keys() {
        rows.push(SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: key.clone(),
        });
    }
    if !pending.env.is_empty() {
        rows.push(SettingsEnvRow::SectionSpacer);
    }
    rows.push(SettingsEnvRow::GlobalAddSentinel);
    for (role, role_env) in &pending.roles {
        if role_env.is_empty() {
            continue;
        }
        rows.push(SettingsEnvRow::SectionSpacer);
        let expanded = expanded_roles.contains(role);
        rows.push(SettingsEnvRow::RoleHeader {
            role: role.clone(),
            expanded,
        });
        if expanded {
            for key in role_env.keys() {
                rows.push(SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role.clone()),
                    key: key.clone(),
                });
            }
            rows.push(SettingsEnvRow::SectionSpacer);
            rows.push(SettingsEnvRow::RoleAddSentinel(role.clone()));
        }
    }
    rows
}

#[must_use]
pub fn settings_env_flat_row_count<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
) -> usize {
    settings_env_flat_rows(pending, expanded_roles).len()
}

#[must_use]
pub fn settings_env_value<'a, V>(
    pending: &'a SettingsEnvConfig<V>,
    scope: &SettingsEnvScope,
    key: &str,
) -> Option<&'a V> {
    match scope {
        SettingsEnvScope::Global => pending.env.get(key),
        SettingsEnvScope::Role(role) => pending
            .roles
            .get(role)
            .and_then(|role_env| role_env.get(key)),
    }
}

#[must_use]
pub fn forbidden_settings_env_keys<V>(
    pending: &SettingsEnvConfig<V>,
    scope: &SettingsEnvScope,
) -> Vec<String> {
    match scope {
        SettingsEnvScope::Global => pending.env.keys().cloned().collect(),
        SettingsEnvScope::Role(role) => pending
            .roles
            .get(role)
            .map(|role_env| role_env.keys().cloned().collect())
            .unwrap_or_default(),
    }
}

pub fn set_settings_env_value<V>(
    pending: &mut SettingsEnvConfig<V>,
    expanded_roles: &mut BTreeSet<String>,
    scope: &SettingsEnvScope,
    key: &str,
    value: V,
) {
    match scope {
        SettingsEnvScope::Global => {
            pending.env.insert(key.to_owned(), value);
        }
        SettingsEnvScope::Role(role) => {
            pending
                .roles
                .entry(role.clone())
                .or_default()
                .insert(key.to_owned(), value);
            expanded_roles.insert(role.clone());
        }
    }
}

pub fn toggle_settings_env_mask_for_row<V>(
    unmasked_rows: &mut BTreeSet<(SettingsEnvScope, String)>,
    pending: &SettingsEnvConfig<V>,
    row: Option<&SettingsEnvRow>,
    is_maskable: impl FnOnce(&V) -> bool,
) -> bool {
    let Some(SettingsEnvRow::Key { scope, key }) = row else {
        return false;
    };
    let Some(value) = settings_env_value(pending, scope, key) else {
        return false;
    };
    if !is_maskable(value) {
        return false;
    }

    let tag = (scope.clone(), key.clone());
    if !unmasked_rows.remove(&tag) {
        unmasked_rows.insert(tag);
    }
    true
}

pub fn toggle_selected_settings_env_mask<V>(
    unmasked_rows: &mut BTreeSet<(SettingsEnvScope, String)>,
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
    is_maskable: impl FnOnce(&V) -> bool,
) -> bool {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    toggle_settings_env_mask_for_row(unmasked_rows, pending, rows.get(selected), is_maskable)
}

pub fn toggle_selected_settings_env_maskable_value(
    unmasked_rows: &mut BTreeSet<(SettingsEnvScope, String)>,
    pending: &SettingsEnvConfig<EnvValue>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> bool {
    toggle_selected_settings_env_mask(unmasked_rows, pending, expanded_roles, selected, |value| {
        !matches!(value, EnvValue::OpRef(_))
    })
}

pub fn remove_settings_env_row<V>(
    pending: &mut SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: &mut usize,
    row: Option<&SettingsEnvRow>,
) -> bool {
    let Some(SettingsEnvRow::Key { scope, key }) = row else {
        return false;
    };
    match scope {
        SettingsEnvScope::Global => {
            pending.env.remove(key);
        }
        SettingsEnvScope::Role(role) => {
            if let Some(role_env) = pending.roles.get_mut(role) {
                role_env.remove(key);
            }
        }
    }
    let row_count = settings_env_flat_row_count(pending, expanded_roles);
    *selected = (*selected).min(row_count.saturating_sub(1));
    true
}

pub fn remove_selected_settings_env_row<V>(
    pending: &mut SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: &mut usize,
) -> bool {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    remove_settings_env_row(pending, expanded_roles, selected, rows.get(*selected))
}

#[must_use]
pub fn settings_env_add_target_for_row(row: Option<&SettingsEnvRow>) -> Option<SettingsEnvScope> {
    match row? {
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            ..
        }
        | SettingsEnvRow::GlobalAddSentinel => Some(SettingsEnvScope::Global),
        SettingsEnvRow::RoleHeader { role, .. }
        | SettingsEnvRow::Key {
            scope: SettingsEnvScope::Role(role),
            ..
        }
        | SettingsEnvRow::RoleAddSentinel(role) => Some(SettingsEnvScope::Role(role.clone())),
        SettingsEnvRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn settings_env_selected_add_target<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> Option<SettingsEnvScope> {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_add_target_for_row(rows.get(selected))
}

#[must_use]
pub fn settings_env_picker_target_for_row(
    row: Option<&SettingsEnvRow>,
) -> Option<(SettingsEnvScope, Option<String>)> {
    match row? {
        SettingsEnvRow::Key { scope, key } => Some((scope.clone(), Some(key.clone()))),
        SettingsEnvRow::GlobalAddSentinel => Some((SettingsEnvScope::Global, None)),
        SettingsEnvRow::RoleAddSentinel(role) => Some((SettingsEnvScope::Role(role.clone()), None)),
        SettingsEnvRow::RoleHeader { .. } | SettingsEnvRow::SectionSpacer => None,
    }
}

#[must_use]
pub fn settings_env_selected_picker_target<V>(
    pending: &SettingsEnvConfig<V>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> Option<(SettingsEnvScope, Option<String>)> {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_picker_target_for_row(rows.get(selected))
}

#[must_use]
pub fn settings_env_enter_plan_for_row<V>(
    pending: &SettingsEnvConfig<V>,
    row: Option<&SettingsEnvRow>,
    can_edit_value: impl FnOnce(Option<&V>) -> bool,
) -> SettingsEnvEnterPlan {
    match row {
        Some(SettingsEnvRow::Key { scope, key }) => {
            let value = settings_env_value(pending, scope, key);
            if can_edit_value(value) {
                SettingsEnvEnterPlan::EditValue {
                    scope: scope.clone(),
                    key: key.clone(),
                }
            } else {
                SettingsEnvEnterPlan::Noop
            }
        }
        Some(SettingsEnvRow::GlobalAddSentinel) => SettingsEnvEnterPlan::OpenScopePicker,
        Some(SettingsEnvRow::RoleHeader {
            role,
            expanded: false,
        }) => SettingsEnvEnterPlan::ExpandRole(role.clone()),
        Some(SettingsEnvRow::RoleAddSentinel(role)) => SettingsEnvEnterPlan::AddRoleKey {
            scope: SettingsEnvScope::Role(role.clone()),
        },
        Some(SettingsEnvRow::RoleHeader { .. } | SettingsEnvRow::SectionSpacer) | None => {
            SettingsEnvEnterPlan::Noop
        }
    }
}

#[must_use]
pub fn settings_env_selected_enter_plan(
    pending: &SettingsEnvConfig<EnvValue>,
    expanded_roles: &BTreeSet<String>,
    selected: usize,
) -> SettingsEnvEnterPlan {
    let rows = settings_env_flat_rows(pending, expanded_roles);
    settings_env_enter_plan_for_row(pending, rows.get(selected), |value| {
        !value.is_some_and(|v| matches!(v, EnvValue::OpRef(_)))
    })
}

#[must_use]
pub fn step_cursor_down_by<F>(candidate: usize, max: usize, mut is_skipped: F) -> usize
where
    F: FnMut(usize) -> bool,
{
    let mut idx = candidate;
    while idx <= max {
        if is_skipped(idx) {
            idx += 1;
        } else {
            return idx;
        }
    }
    candidate
}

#[must_use]
pub fn step_cursor_up_by<F>(candidate: usize, mut is_skipped: F) -> usize
where
    F: FnMut(usize) -> bool,
{
    let mut idx = candidate;
    loop {
        if is_skipped(idx) {
            if idx == 0 {
                return 0;
            }
            idx -= 1;
        } else {
            return idx;
        }
    }
}

#[must_use]
pub fn settings_vec_change_count<T: PartialEq>(original: &[T], pending: &[T]) -> usize {
    let common_changes = original
        .iter()
        .zip(pending.iter())
        .filter(|(a, b)| a != b)
        .count();
    common_changes + original.len().abs_diff(pending.len())
}

#[must_use]
pub fn settings_map_change_count<V: PartialEq>(
    original: &BTreeMap<String, V>,
    pending: &BTreeMap<String, V>,
) -> usize {
    let mut count = 0;
    for (key, value) in pending {
        match original.get(key) {
            None => count += 1,
            Some(original_value) if original_value != value => count += 1,
            _ => {}
        }
    }
    for key in original.keys() {
        if !pending.contains_key(key) {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests;
