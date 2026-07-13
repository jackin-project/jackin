// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// `SettingsGeneralState` impls + `settings_general` helpers.
use super::{
    SettingsGeneralSaveRefs, SettingsGeneralState, SettingsPanelChangeCount, SettingsPanelDirty,
    SettingsPanelDiscard, SettingsPanelMarkSaved,
};

impl SettingsGeneralState {
    #[must_use]
    pub const fn from_values(coauthor_trailer: bool, dco: bool) -> Self {
        Self {
            pending_coauthor_trailer: coauthor_trailer,
            original_coauthor_trailer: coauthor_trailer,
            pending_dco: dco,
            original_dco: dco,
            selected: 0,
        }
    }

    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.pending_coauthor_trailer != self.original_coauthor_trailer
            || self.pending_dco != self.original_dco
    }

    #[must_use]
    pub const fn save_refs(&self) -> SettingsGeneralSaveRefs {
        SettingsGeneralSaveRefs {
            git_coauthor_trailer: self.pending_coauthor_trailer,
            git_dco: self.pending_dco,
        }
    }

    pub const fn discard(&mut self) {
        self.pending_coauthor_trailer = self.original_coauthor_trailer;
        self.pending_dco = self.original_dco;
    }

    #[must_use]
    pub fn change_count(&self) -> usize {
        usize::from(self.pending_coauthor_trailer != self.original_coauthor_trailer)
            + usize::from(self.pending_dco != self.original_dco)
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = crate::tui::focus::moved_selection(self.selected, 2, delta);
    }

    pub const fn toggle_selected(&mut self) {
        match self.selected {
            0 => {
                self.pending_coauthor_trailer = !self.pending_coauthor_trailer;
            }
            1 => {
                self.pending_dco = !self.pending_dco;
            }
            _ => {}
        }
    }

    pub const fn mark_clean(&mut self) {
        self.original_coauthor_trailer = self.pending_coauthor_trailer;
        self.original_dco = self.pending_dco;
    }
}

impl SettingsPanelDirty for SettingsGeneralState {
    fn panel_is_dirty(&self) -> bool {
        self.is_dirty()
    }
}

impl SettingsPanelChangeCount for SettingsGeneralState {
    fn panel_change_count(&self) -> usize {
        self.change_count()
    }
}

impl SettingsPanelDiscard for SettingsGeneralState {
    fn panel_discard(&mut self) {
        self.discard();
    }
}

impl SettingsPanelMarkSaved for SettingsGeneralState {
    fn panel_mark_saved(&mut self) {
        self.mark_clean();
    }
}
