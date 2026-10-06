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

mod support_01;
use support_01::*;
mod support_02;
use support_02::*;
mod support_03;
use support_03::*;
mod case_01;
mod case_02;
