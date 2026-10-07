// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings focus region plans.

use super::super::model::{SettingsHoverTarget, SettingsTab};

/// Focus regions of the settings screen, in the order the key-driven focus
/// cycle walks them — copy-adapted from the upstream
/// `patterns/settings_screen.rs` recipe's `SettingsRegion::focus_order()`
/// (composition reference, never a type dependency): category navigation
/// before body. The recipe's Search and Footer regions have no console
/// counterpart (no settings search exists; the footer hint bar is
/// chrome-only), and its KeybindingRecorder/ThemePicker integrations are
/// not copy-adapted (N4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFocusRegion {
    /// Category navigation: the tab bar (recipe: Nav).
    TabBar,
    /// Active tab's form section (recipe: Body).
    Content,
}

/// The ordered focus-region chain — the recipe's `focus_order()` walk.
#[must_use]
pub const fn settings_focus_order() -> [SettingsFocusRegion; 2] {
    [SettingsFocusRegion::TabBar, SettingsFocusRegion::Content]
}

/// Head of the focus chain: the cycle's start and the exits' return target.
#[must_use]
pub const fn settings_focus_head() -> SettingsFocusRegion {
    let [head, ..] = settings_focus_order();
    head
}

/// The region after `region` in the chain, wrapping to the head at the end.
#[must_use]
pub const fn settings_focus_next(region: SettingsFocusRegion) -> SettingsFocusRegion {
    let [head, second] = settings_focus_order();
    match region {
        SettingsFocusRegion::TabBar => second,
        SettingsFocusRegion::Content => head,
    }
}

/// The region currently owning focus, derived from the tab-bar flag.
#[must_use]
pub const fn settings_focus_region(tab_bar_focused: bool) -> SettingsFocusRegion {
    if tab_bar_focused {
        SettingsFocusRegion::TabBar
    } else {
        SettingsFocusRegion::Content
    }
}

/// Region identity without `PartialEq` (usable in `const fn`).
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
