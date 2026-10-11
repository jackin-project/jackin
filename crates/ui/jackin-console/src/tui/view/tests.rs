// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

// Tests for `view`.

use super::*;

use crate::tui::model::{ConsoleManagerStageRoute, ConsoleStageModalFacts};

use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect};

use crate::tui::{
    state::{
        EditorState, EditorTab, GlobalMountConfirm, ManagerStage, ManagerState, Modal,
        MountScrollFocus, SettingsEnvScope, SettingsEnvTextTarget, SettingsModal, SettingsState,
    },
    view::{prepare_for_render, render},
};

use jackin_config::{AppConfig, WorkspaceConfig};


mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
