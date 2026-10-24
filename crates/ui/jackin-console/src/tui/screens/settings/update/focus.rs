// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
