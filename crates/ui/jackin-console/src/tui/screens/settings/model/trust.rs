// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Trust settings state.

use crate::tui::components::footer_hints::ModalFooterMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsTrustRow {
    pub role: String,
    pub git: String,
    pub trusted: bool,
}

#[derive(Debug)]
pub struct SettingsTrustState {
    pub selected: usize,
    pub pending: Vec<SettingsTrustRow>,
    pub original: Vec<SettingsTrustRow>,
    pub error: Option<String>,
    pub scroll: termrock::widgets::ScrollAreaState,
}

#[derive(Debug, Clone, Copy)]
pub struct SettingsTrustSaveRefs<'a> {
    pub pending: &'a [SettingsTrustRow],
}

pub(crate) fn footer_items_for_mode(
    mode: ModalFooterMode,
) -> Vec<termrock::widgets::HintSpan<'static>> {
    crate::tui::components::footer_hints::modal_footer_items(mode)
}
