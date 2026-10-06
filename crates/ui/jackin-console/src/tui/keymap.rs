// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Console keymaps — single source of truth coupling key dispatch and hint-bar
//! advertisement for all host-console surfaces.
//!
//! Every keyboard-driven surface (editor, settings tabs, inline picker) defines
//! its keymap here. `Keymap::dispatch(chord)` replaces plan-function calls in
//! `input/*.rs`; `Keymap::hint_spans()` derives footer hints.
mod bridge;
mod editor;
mod micros;
mod preview_console;
mod settings_env;
mod settings_general;
mod settings_mounts;
mod settings_shell;
mod settings_trust;
#[cfg(test)]
mod tests;
mod workspace;
pub(crate) use bridge::bridged_keymap_action;
pub(crate) use editor::{
    EDITOR_CONTENT_KEYMAP, EDITOR_GLOBAL_KEYMAP, EDITOR_TAB_BAR_KEYMAP, EditorContentAction,
    EditorGlobalAction, EditorTabBarAction,
};
pub(crate) use micros::{
    AUTH_EDIT_SOURCE_KEYMAP, AUTH_MANAGE_KEYMAP, EDITOR_GENERAL_RENAME_KEYMAP,
    EDITOR_GENERAL_TOGGLE_KEYMAP, EDITOR_GENERAL_WORKDIR_KEYMAP, EDITOR_ROLE_NEW_KEYMAP,
    INLINE_PICKER_SHELL_KEYMAP, InlinePickerShellAction, SETTINGS_GENERAL_TOGGLE_KEYMAP,
    SETTINGS_TRUST_TOGGLE_KEYMAP,
};
pub(crate) use preview_console::{
    CONSOLE_GLOBAL_KEYMAP, ConsoleGlobalAction, PREVIEW_PANE_KEYMAP, PreviewPaneAction,
};
pub(crate) use settings_env::{SETTINGS_ENV_TAB_KEYMAP, SettingsEnvTabAction};
pub(crate) use settings_general::{SETTINGS_GENERAL_TAB_KEYMAP, SettingsGeneralTabAction};
pub(crate) use settings_mounts::{
    SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, SettingsGlobalMountsTabAction,
};
pub(crate) use settings_shell::{
    SETTINGS_CONTENT_SHELL_KEYMAP, SETTINGS_TAB_BAR_KEYMAP, SettingsContentShellAction,
    SettingsTabBarAction,
};
pub(crate) use settings_trust::{SETTINGS_TRUST_TAB_KEYMAP, SettingsTrustTabAction};
pub(crate) use workspace::{WORKSPACE_LIST_KEYMAP, WorkspaceListAction};
