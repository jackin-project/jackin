// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    EDITOR_CONTENT_KEYMAP, EDITOR_GENERAL_RENAME_KEYMAP, EDITOR_GENERAL_TOGGLE_KEYMAP,
    EDITOR_GENERAL_WORKDIR_KEYMAP, EDITOR_GLOBAL_KEYMAP, EDITOR_ROLE_NEW_KEYMAP,
    EDITOR_TAB_BAR_KEYMAP, EditorContentAction, EditorGlobalAction, EditorTabBarAction,
    INLINE_PICKER_SHELL_KEYMAP, InlinePickerShellAction, PREVIEW_PANE_KEYMAP, PreviewPaneAction,
    SETTINGS_CONTENT_SHELL_KEYMAP, SETTINGS_ENV_TAB_KEYMAP, SETTINGS_GENERAL_TAB_KEYMAP,
    SETTINGS_GENERAL_TOGGLE_KEYMAP, SETTINGS_GLOBAL_MOUNTS_TAB_KEYMAP, SETTINGS_TAB_BAR_KEYMAP,
    SETTINGS_TRUST_TAB_KEYMAP, SETTINGS_TRUST_TOGGLE_KEYMAP, SettingsContentShellAction,
    SettingsEnvTabAction, SettingsGeneralTabAction, SettingsGlobalMountsTabAction,
    SettingsTabBarAction, SettingsTrustTabAction, WORKSPACE_LIST_KEYMAP, WorkspaceListAction,
};

use termrock::input::KeyCode;

use termrock::keymap::KeyChord;

use super::bridged_keymap_action;

use termrock::input::{KeyEvent, KeyModifiers};

use termrock::keymap::Keymap as TermrockKeymap;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
