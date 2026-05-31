use std::collections::BTreeMap;

use super::state::{SettingsGeneralState, SettingsTab};

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

pub fn move_general_selection(state: &mut SettingsGeneralState, delta: isize) {
    state.selected = if delta.is_negative() {
        state.selected.saturating_sub(delta.unsigned_abs())
    } else {
        state.selected.saturating_add(delta as usize).min(1)
    };
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
