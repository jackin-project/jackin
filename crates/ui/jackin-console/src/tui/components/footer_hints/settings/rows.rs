// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings row and context footer items.

use super::{
    global_mount_row_footer_items, secret_add_row_footer_items, secret_op_ref_row_footer_items,
    secret_plain_row_footer_items, secret_role_header_footer_items,
};
use termrock::scroll::ScrollAxes;
use termrock::widgets::HintSpan;

use crate::tui::keymap::{SETTINGS_GENERAL_TOGGLE_KEYMAP, SETTINGS_TRUST_TOGGLE_KEYMAP};
use termrock::scroll::scroll_hint_spans;

#[must_use]
pub fn settings_general_row_footer_items() -> Vec<HintSpan<'static>> {
    // `content_footer_items` already prepends ↑↓ navigate; only add the tab-specific action.
    SETTINGS_GENERAL_TOGGLE_KEYMAP.hint_spans()
}

#[must_use]
pub fn settings_trust_row_footer_items(
    has_roles: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    if has_roles {
        let mut items = SETTINGS_TRUST_TOGGLE_KEYMAP.hint_spans();
        let scroll_items = scroll_hint_spans(scroll_axes);
        if !scroll_items.is_empty() {
            items.push(HintSpan::Sep);
            items.extend(scroll_items);
        }
        items
    } else {
        Vec::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsContextFooterMode {
    General,
    MountRow {
        has_github_url: bool,
        scroll_axes: ScrollAxes,
    },
    MountAddRow,
    EnvOpRefRow,
    EnvPlainRow,
    EnvRoleHeader,
    EnvAddRow,
    Empty,
    AuthManage,
    AuthEditMode,
    AuthEditSource,
    Trust {
        has_roles: bool,
        scroll_axes: ScrollAxes,
    },
}

#[must_use]
pub fn settings_contextual_row_footer_items(
    mode: SettingsContextFooterMode,
    op_available: bool,
) -> Vec<HintSpan<'static>> {
    match mode {
        SettingsContextFooterMode::General => settings_general_row_footer_items(),
        SettingsContextFooterMode::MountRow {
            has_github_url,
            scroll_axes,
        } => global_mount_row_footer_items(has_github_url, scroll_axes),
        SettingsContextFooterMode::MountAddRow => add_row_footer_items("add"),
        SettingsContextFooterMode::EnvOpRefRow => secret_op_ref_row_footer_items(op_available),
        SettingsContextFooterMode::EnvPlainRow => secret_plain_row_footer_items(op_available),
        SettingsContextFooterMode::EnvRoleHeader => secret_role_header_footer_items(),
        SettingsContextFooterMode::EnvAddRow => secret_add_row_footer_items(op_available),
        SettingsContextFooterMode::Empty => Vec::new(),
        SettingsContextFooterMode::AuthManage => {
            vec![
                super::super::key_span("↵"),
                HintSpan::Text("add/edit"),
                HintSpan::Sep,
                super::super::key_span("D"),
                HintSpan::Text("remove"),
                HintSpan::Sep,
                super::super::key_span("E"),
                HintSpan::Text("enable/disable"),
                HintSpan::Sep,
                super::super::key_span("F"),
                HintSpan::Text("default for agent"),
                HintSpan::Sep,
                super::super::key_span("R"),
                HintSpan::Text("rename"),
                HintSpan::Sep,
                super::super::key_span("B"),
                HintSpan::Text("base URL"),
                HintSpan::Sep,
                super::super::key_span("M"),
                HintSpan::Text("model"),
            ]
        }
        SettingsContextFooterMode::AuthEditMode => super::super::editor::auth_row_footer_items(
            super::super::editor::AuthRowFooterMode::EditMode,
        ),
        SettingsContextFooterMode::AuthEditSource => super::super::editor::auth_row_footer_items(
            super::super::editor::AuthRowFooterMode::EditSource,
        ),
        SettingsContextFooterMode::Trust {
            has_roles,
            scroll_axes,
        } => settings_trust_row_footer_items(has_roles, scroll_axes),
    }
}

#[must_use]
pub fn add_row_footer_items(label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group): combined Enter/A display; Enter and A are separate chords.
        super::super::key_span("↵/A"),
        HintSpan::Text(label),
    ]
}

pub fn append_generate_token_footer_item(items: &mut Vec<HintSpan<'static>>) {
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(auth-form-no-keymap): G triggers token generation inline; no AUTH_FORM_KEYMAP.
        super::super::key_span("G"),
        HintSpan::Text("generate"),
    ]);
}
