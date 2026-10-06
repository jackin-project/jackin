// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Settings mount and secret-row footer items.

use super::super::common::append_open_in_github;
use crate::tui::keymap::{
    SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, SettingsEnvTabAction,
    SettingsGlobalMountsTabAction,
};
use termrock::scroll::ScrollAxes;
use termrock::scroll::scroll_hint_spans;
use termrock::{keymap::glyph, widgets::HintSpan};

#[must_use]
pub fn workspace_mount_row_footer_items(
    has_github_url: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(workspace-mount-row-no-keymap): D removes mount inline; no WORKSPACE_MOUNT_ROW_KEYMAP.
        super::super::key_span("D"),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): A adds mount inline.
        super::super::key_span("A"),
        HintSpan::Text("add"),
    ];
    append_open_in_github(&mut items, has_github_url);
    items.extend([
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): R toggles read-only inline.
        super::super::key_span("R"),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): I cycles isolation inline.
        super::super::key_span("I"),
        HintSpan::Text("cycle isolation"),
    ]);
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::Sep);
        items.extend(scroll_items);
    }
    items
}

#[must_use]
pub fn global_mount_row_footer_items(
    has_github_url: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        super::super::key_span(g(SettingsGlobalMountsTabAction::Delete)),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsGlobalMountsTabAction::Add)),
        HintSpan::Text("add"),
    ];
    if has_github_url {
        items.extend([
            HintSpan::Sep,
            super::super::key_span(g(SettingsGlobalMountsTabAction::OpenGithub)),
            HintSpan::Text("open in GitHub"),
        ]);
    }
    items.extend([
        HintSpan::Sep,
        super::super::key_span(g(SettingsGlobalMountsTabAction::ToggleReadonly)),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsGlobalMountsTabAction::EditRename)),
        HintSpan::Text("rename"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsGlobalMountsTabAction::EditSource)),
        HintSpan::Text("edit source"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsGlobalMountsTabAction::EditDest)),
        HintSpan::Text("edit dst"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsGlobalMountsTabAction::EditScope)),
        HintSpan::Text("edit scope"),
    ]);
    let scroll_items = scroll_hint_spans(scroll_axes);
    if !scroll_items.is_empty() {
        items.push(HintSpan::Sep);
        items.extend(scroll_items);
    }
    items
}

#[must_use]
pub fn secret_op_ref_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = if op_available {
        vec![
            super::super::key_span(g(SettingsEnvTabAction::Enter)),
            HintSpan::Sep,
            super::super::key_span(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("re-pick from 1Password"),
            HintSpan::Sep,
        ]
    } else {
        Vec::new()
    };
    items.extend([
        super::super::key_span(g(SettingsEnvTabAction::Delete)),
        HintSpan::Text("delete"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
    ]);
    items
}

#[must_use]
pub fn secret_plain_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        super::super::key_span(g(SettingsEnvTabAction::Enter)),
        HintSpan::Text("edit"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsEnvTabAction::Delete)),
        HintSpan::Text("delete"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
        HintSpan::Sep,
        super::super::key_span(g(SettingsEnvTabAction::ToggleMask)),
        HintSpan::Text("mask/unmask"),
    ];
    if op_available {
        items.extend([
            HintSpan::Sep,
            super::super::key_span(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("1Password"),
        ]);
    }
    items
}

#[must_use]
pub fn secret_add_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        super::super::key_span(g(SettingsEnvTabAction::Enter)),
        HintSpan::Text("add"),
    ];
    if op_available {
        items.extend([
            HintSpan::Sep,
            super::super::key_span(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("1Password"),
        ]);
    }
    items
}

#[must_use]
pub fn secret_role_header_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        super::super::key_span(SETTINGS_ENV_TAB_KEYMAP.glyph_for(SettingsEnvTabAction::Enter)),
        HintSpan::Text("expand"),
        HintSpan::Sep,
        // UNREGISTERABLE(multi-key-display-group): combined collapse/expand left/right display.
        super::super::key_span(glyph::LEFT_RIGHT),
        HintSpan::Text("collapse/expand"),
        HintSpan::Sep,
        super::super::key_span(SETTINGS_ENV_TAB_KEYMAP.glyph_for(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
    ]
}
