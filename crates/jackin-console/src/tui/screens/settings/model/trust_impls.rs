// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// `SettingsTrustState` impls + `settings_trust_rows_from_app_config`.
use super::{
    SettingsPanelChangeCount, SettingsPanelDirty, SettingsPanelDiscard, SettingsPanelMarkSaved,
    SettingsPanelTakeError, SettingsTrustRow, SettingsTrustSaveRefs, SettingsTrustState,
};

impl SettingsTrustState {
    #[must_use]
    pub fn from_config(config: &jackin_config::AppConfig) -> Self {
        Self::from_rows(settings_trust_rows_from_app_config(config))
    }

    #[must_use]
    pub fn from_rows(pending: Vec<SettingsTrustRow>) -> Self {
        Self {
            selected: 0,
            original: pending.clone(),
            pending,
            error: None,
            scroll_x: 0,
            scroll_y: 0,
        }
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.pending != self.original
    }

    #[must_use]
    pub fn save_refs(&self) -> SettingsTrustSaveRefs<'_> {
        SettingsTrustSaveRefs {
            pending: &self.pending,
        }
    }

    pub fn discard(&mut self) {
        self.pending = self.original.clone();
        self.selected = self.selected.min(self.pending.len().saturating_sub(1));
        self.error = None;
    }

    pub fn apply_selection_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsSelectionScrollPlan,
    ) {
        self.selected = plan.selected;
        self.scroll_y = plan.scroll_y;
    }

    pub fn apply_row_select_plan(
        &mut self,
        plan: crate::tui::screens::settings::update::SettingsTrustRowSelectPlan,
    ) -> bool {
        if let Some(selected) = plan.selected {
            self.selected = selected;
        }
        plan.content_focused
    }

    pub fn apply_horizontal_scroll(&mut self, scroll_x: u16) {
        self.scroll_x = scroll_x;
    }

    pub fn toggle_selected(&mut self) {
        if let Some(row) = self.pending.get_mut(self.selected) {
            row.trusted = !row.trusted;
        }
    }

    pub fn mark_saved(&mut self) {
        self.original = self.pending.clone();
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

impl SettingsPanelTakeError for SettingsTrustState {
    fn take_panel_error(&mut self) -> Option<String> {
        self.take_error()
    }
}

impl SettingsPanelDirty for SettingsTrustState {
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl SettingsPanelChangeCount for SettingsTrustState {
    fn panel_change_count(&self) -> usize {
        crate::tui::screens::settings::update::settings_vec_change_count(
            &self.original,
            &self.pending,
        )
    }
}

impl SettingsPanelDiscard for SettingsTrustState {
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl SettingsPanelMarkSaved for SettingsTrustState {
    fn panel_mark_saved(&mut self) {
        self.mark_saved();
    }
}

#[must_use]
pub fn settings_trust_rows_from_app_config(
    config: &jackin_config::AppConfig,
) -> Vec<SettingsTrustRow> {
    config
        .roles
        .iter()
        .map(|(role, source)| SettingsTrustRow {
            role: role.clone(),
            git: source.git.clone(),
            trusted: source.trusted,
        })
        .collect()
}
