// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings aggregate state and slots.

use super::{SettingsGeneralState, SettingsTab};

use crate::tui::focus::TabFocus;

#[derive(Debug)]
pub struct SettingsState<Mounts, Env, Auth, Trust, ErrorPopup> {
    pub active_tab: SettingsTab,
    /// W3C ARIA Tabs: focus is either on the tab list or the active tab panel.
    pub focus_owner: TabFocus<SettingsTab>,
    pub hover_target: Option<SettingsHoverTarget>,
    pub general: SettingsGeneralState,
    pub mounts: Mounts,
    pub env: Env,
    pub auth: Auth,
    pub trust: Trust,
    /// Error popup shown on top of all settings content.
    pub error_popup: Option<ErrorPopup>,
    /// Token-generate request drained by the run loop.

    /// Cached footer height for mouse hit-testing.
    pub cached_footer_h: u16,
}

pub trait SettingsPanelTakeError {
    fn take_panel_error(&mut self) -> Option<String>;
}

pub trait SettingsAuthRestorePendingForm {
    fn restore_pending_auth_form(&mut self);
}

pub trait SettingsMountsTakeExit {
    fn take_mounts_exit_requested(&mut self) -> bool;
}

pub trait SettingsModalSlot {
    type Modal;

    fn modal_mut(&mut self) -> Option<&mut Self::Modal>;
}

pub trait SettingsAuthSlot {
    type Modal;

    fn modal_mut(&mut self) -> Option<&mut Self::Modal>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsAfterEventOutcome {
    pub exit_requested: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsHoverTarget {
    Tab(usize),
    TrustRow(usize),
}
