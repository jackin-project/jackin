// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `view`.

use super::super::model;
use super::*;

use crate::tui::components::editor_rows::AuthSourceDisplay;

use crate::tui::components::editor_rows::{AuthSourceFolderDisplay, AuthSourceFolderKind};

use crate::tui::state::SettingsTab;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
