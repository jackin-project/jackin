// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Settings + mount + secret + auth-form hint-span builders for the
//! settings screen's per-row contextual footer.

use jackin_tui::HintSpan;
use jackin_tui::components::ScrollAxes;

use super::common::append_open_in_github;
use crate::tui::keymap::{
    SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GENERAL_TOGGLE_KEYMAP, SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP,
    SETTINGS_TRUST_TOGGLE_KEYMAP, SettingsEnvTabAction, SettingsGlobalMountsTabAction,
};
use jackin_tui::components::scroll_hint_spans;

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
            super::editor::auth_row_footer_items(super::editor::AuthRowFooterMode::ManageAuth)
        }
        SettingsContextFooterMode::AuthEditMode => {
            super::editor::auth_row_footer_items(super::editor::AuthRowFooterMode::EditMode)
        }
        SettingsContextFooterMode::AuthEditSource => {
            super::editor::auth_row_footer_items(super::editor::AuthRowFooterMode::EditSource)
        }
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
        HintSpan::Key("↵/A"),
        HintSpan::Text(label),
    ]
}

pub fn append_generate_token_footer_item(items: &mut Vec<HintSpan<'static>>) {
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(auth-form-no-keymap): G triggers token generation inline; no AUTH_FORM_KEYMAP.
        HintSpan::Key("G"),
        HintSpan::Text("generate"),
    ]);
}

#[must_use]
pub fn mount_destination_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(mount-destination-no-keymap): M handled inline; no MOUNT_DESTINATION_KEYMAP.
        HintSpan::Key("M"),
        HintSpan::Text("mount"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(mount-destination-no-keymap): E handled inline.
        HintSpan::Key("E"),
        HintSpan::Text("edit"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined left/right display.
        HintSpan::Key("←/→"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(mount-destination-no-keymap): Enter confirms inline.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(multi-key-display-group): combined C/Esc cancel display.
        HintSpan::Key("C/Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn segmented_choice_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("←/→"),
        HintSpan::Text("move"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(segmented-choice-no-keymap): Enter handled inline; no SEGMENTED_CHOICE_KEYMAP.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(segmented-choice-no-keymap): Esc handled inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn pick_list_footer_items(commit_label: &'static str) -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(pick-list-no-keymap): Enter handled inline; no PICK_LIST_KEYMAP.
        HintSpan::Key("↵"),
        HintSpan::Text(commit_label),
        HintSpan::GroupSep,
        // UNREGISTERABLE(pick-list-no-keymap): Esc handled inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn filtered_picker_footer_items(
    include_refresh: bool,
    include_collapse: bool,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(descriptive-label): not a key — describes free-text filter input.
        HintSpan::Key("type"),
        HintSpan::Text("filter"),
    ];
    if include_refresh {
        items.extend([
            HintSpan::GroupSep,
            // UNREGISTERABLE(filtered-picker-no-keymap): R refresh handled inline; no FILTERED_PICKER_KEYMAP.
            HintSpan::Key("R"),
            HintSpan::Text("refresh"),
        ]);
    }
    if include_collapse {
        items.extend([
            HintSpan::GroupSep,
            // UNREGISTERABLE(multi-key-display-group)
            HintSpan::Key("←/→"),
            HintSpan::Text("collapse/expand section"),
        ]);
    }
    items.extend([
        HintSpan::GroupSep,
        // UNREGISTERABLE(filtered-picker-no-keymap): Enter selects inline.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(filtered-picker-no-keymap): Esc cancels inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]);
    items
}

#[must_use]
pub fn op_section_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        // UNREGISTERABLE(multi-key-display-group)
        HintSpan::Key("↑↓"),
        HintSpan::Text("navigate"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(op-section-no-keymap): Enter handled inline; no OP_SECTION_KEYMAP.
        HintSpan::Key("↵"),
        HintSpan::Text("select"),
        HintSpan::GroupSep,
        // UNREGISTERABLE(op-section-no-keymap): Esc handled inline.
        HintSpan::Key("Esc"),
        HintSpan::Text("cancel"),
    ]
}

#[must_use]
pub fn workspace_mount_row_footer_items(
    has_github_url: bool,
    scroll_axes: ScrollAxes,
) -> Vec<HintSpan<'static>> {
    let mut items = vec![
        // UNREGISTERABLE(workspace-mount-row-no-keymap): D removes mount inline; no WORKSPACE_MOUNT_ROW_KEYMAP.
        HintSpan::Key("D"),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): A adds mount inline.
        HintSpan::Key("A"),
        HintSpan::Text("add"),
    ];
    append_open_in_github(&mut items, has_github_url);
    items.extend([
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): R toggles read-only inline.
        HintSpan::Key("R"),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::Sep,
        // UNREGISTERABLE(workspace-mount-row-no-keymap): I cycles isolation inline.
        HintSpan::Key("I"),
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
        HintSpan::Key(g(SettingsGlobalMountsTabAction::Delete)),
        HintSpan::Text("remove"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::Add)),
        HintSpan::Text("add"),
    ];
    if has_github_url {
        items.extend([
            HintSpan::Sep,
            HintSpan::Key(g(SettingsGlobalMountsTabAction::OpenGithub)),
            HintSpan::Text("open in GitHub"),
        ]);
    }
    items.extend([
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::ToggleReadonly)),
        HintSpan::Text("toggle ro/rw"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditRename)),
        HintSpan::Text("rename"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditSource)),
        HintSpan::Text("edit source"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditDest)),
        HintSpan::Text("edit dst"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsGlobalMountsTabAction::EditScope)),
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
            HintSpan::Key(g(SettingsEnvTabAction::Enter)),
            HintSpan::Sep,
            HintSpan::Key(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("re-pick from 1Password"),
            HintSpan::Sep,
        ]
    } else {
        Vec::new()
    };
    items.extend([
        HintSpan::Key(g(SettingsEnvTabAction::Delete)),
        HintSpan::Text("delete"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
    ]);
    items
}

#[must_use]
pub fn secret_plain_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        HintSpan::Key(g(SettingsEnvTabAction::Enter)),
        HintSpan::Text("edit"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::Delete)),
        HintSpan::Text("delete"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
        HintSpan::Sep,
        HintSpan::Key(g(SettingsEnvTabAction::ToggleMask)),
        HintSpan::Text("mask/unmask"),
    ];
    if op_available {
        items.extend([
            HintSpan::Sep,
            HintSpan::Key(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("1Password"),
        ]);
    }
    items
}

#[must_use]
pub fn secret_add_row_footer_items(op_available: bool) -> Vec<HintSpan<'static>> {
    let g = |a| SETTINGS_ENV_TAB_KEYMAP.glyph_for(a);
    let mut items = vec![
        HintSpan::Key(g(SettingsEnvTabAction::Enter)),
        HintSpan::Text("add"),
    ];
    if op_available {
        items.extend([
            HintSpan::Sep,
            HintSpan::Key(g(SettingsEnvTabAction::OpenPicker)),
            HintSpan::Text("1Password"),
        ]);
    }
    items
}

#[must_use]
pub fn secret_role_header_footer_items() -> Vec<HintSpan<'static>> {
    vec![
        HintSpan::Key(SETTINGS_ENV_TAB_KEYMAP.glyph_for(SettingsEnvTabAction::Enter)),
        HintSpan::Text("expand"),
        HintSpan::Sep,
        // UNREGISTERABLE(multi-key-display-group): combined collapse/expand left/right display.
        HintSpan::Key("←/→"),
        HintSpan::Text("collapse/expand"),
        HintSpan::Sep,
        HintSpan::Key(SETTINGS_ENV_TAB_KEYMAP.glyph_for(SettingsEnvTabAction::Add)),
        HintSpan::Text("add"),
    ]
}

// `auth_form_footer_items` lives in `super::modals` because it is part of
// the `ModalFooterMode::AuthForm` arm and is dispatched from `modal_footer_items`.
