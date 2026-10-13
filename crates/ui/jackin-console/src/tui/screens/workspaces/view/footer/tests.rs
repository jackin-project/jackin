// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_config::{AppConfig, WorkspaceConfig};

use ratatui::layout::Rect;

use std::path::PathBuf;

use termrock::{keymap::glyph, widgets::HintSpan};

use super::workspace_screen_footer_items_for_state;

use crate::tui::components::file_browser::FileBrowserState;

use crate::tui::components::footer_hints::editor_footer_items;

use crate::tui::model::ConsoleManagerStage;

use crate::tui::screens::settings::view::settings_screen_footer_for_state;

use crate::tui::state::{
    CreatePreludeState, FileBrowserTarget, ManagerState, Modal, SettingsModal, SettingsState,
    SettingsTab,
};

mod support;
use support::*;
mod case_01;
