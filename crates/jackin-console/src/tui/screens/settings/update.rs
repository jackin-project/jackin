//! Settings screen update logic: handle keyboard events and produce effects
//! for the General, Mounts, Environments, Auth, and Trust tab group.
//!
//! Not responsible for: rendering (see `view`) or state definitions (see
//! `model`).

use std::collections::{BTreeMap, BTreeSet};

use super::model::{
    GlobalMountConfirm, SettingsEnvConfig, SettingsEnvEnterPlan, SettingsEnvRow, SettingsEnvScope,
    SettingsGeneralState, SettingsTab, SettingsTrustState,
};
use crate::tui::auth::{AuthKind, AuthMode, auth_mode_requires_credential};
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

pub fn move_general_selection(state: &mut SettingsGeneralState, delta: isize) {
    state.selected = crate::tui::focus::moved_selection(state.selected, 2, delta);
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

pub fn toggle_general_selected(state: &mut SettingsGeneralState) {
    match state.selected {
        0 => {
            state.pending_coauthor_trailer = !state.pending_coauthor_trailer;
        }
        1 => {
            state.pending_dco = !state.pending_dco;
        }
        _ => {}
    }
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

pub fn toggle_trust_selected(state: &mut SettingsTrustState) {
    if let Some(row) = state.pending.get_mut(state.selected) {
        row.trusted = !row.trusted;
    }
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

pub fn toggle_readonly(readonly: &mut bool) {
    *readonly = !*readonly;
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
