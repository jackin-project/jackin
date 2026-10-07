// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings panel capability traits and general state.

pub trait SettingsPanelDirty {
    fn panel_is_dirty(&self) -> bool;
}

pub trait SettingsPanelChangeCount {
    fn panel_change_count(&self) -> usize;
}

pub trait SettingsPanelDiscard {
    fn panel_discard(&mut self);
}

pub trait SettingsPanelMarkSaved {
    fn panel_mark_saved(&mut self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub struct SettingsGeneralState {
    pub pending_coauthor_trailer: bool,
    pub original_coauthor_trailer: bool,
    pub pending_dco: bool,
    pub original_dco: bool,
    pub selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingsGeneralSaveRefs {
    pub git_coauthor_trailer: bool,
    pub git_dco: bool,
}
