// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::fixtures::{key, mount};

use super::*;

use crate::console::adapter::state::{
    EditorState, FieldFocus, ManagerStage, ManagerState, SettingsState,
};

use crossterm::event::KeyCode;

use jackin_config::AppConfig;

use jackin_core::JackinPaths;

mod case_01;
