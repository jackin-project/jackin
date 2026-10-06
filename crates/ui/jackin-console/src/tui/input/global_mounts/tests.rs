// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `global_mounts`.

use super::super::fixtures::key;

use super::*;

use crate::tui::components::file_browser::FileBrowserState;

use crate::tui::state::{
    ManagerStage, ManagerState, SettingsEnvRow, SettingsEnvTextTarget, SettingsModal,
    SettingsState, SettingsTab,
};

use jackin_config::{AppConfig, RoleSource};

use jackin_core::JackinPaths;

use ratatui::layout::Rect;

use std::collections::BTreeMap;

use support::confirm_modal;
mod case_01;
mod case_02;
mod case_03;
mod support;
