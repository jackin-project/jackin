// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::model::{SettingsHoverTarget, SettingsTab};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFocusRegion {
    TabBar,
    Content,
}

#[must_use]
pub const fn settings_focus_order() -> [SettingsFocusRegion; 2] {
    [SettingsFocusRegion::TabBar, SettingsFocusRegion::Content]
}

#[must_use]
pub const fn settings_focus_head() -> SettingsFocusRegion {
    let [head, ..] = settings_focus_order();
    head
}

#[must_use]
pub const fn settings_focus_next(region: SettingsFocusRegion) -> SettingsFocusRegion {
    let [head, second] = settings_focus_order();
    match region {
        SettingsFocusRegion::TabBar => second,
        SettingsFocusRegion::Content => head,
    }
}

#[must_use]
pub const fn settings_focus_region(tab_bar_focused: bool) -> SettingsFocusRegion {
    if tab_bar_focused {
        SettingsFocusRegion::TabBar
    } else {
        SettingsFocusRegion::Content
    }
}

#[must_use]
pub(crate) const fn settings_focus_region_eq(
    a: SettingsFocusRegion,
    b: SettingsFocusRegion,
) -> bool {
    matches!(
        (a, b),
        (SettingsFocusRegion::TabBar, SettingsFocusRegion::TabBar)
            | (SettingsFocusRegion::Content, SettingsFocusRegion::Content)
    )
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
