// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::auth::AuthKind;

use crate::tui::components::ErrorPopupState;

use crate::tui::focus::ConsoleFocusTarget;

use crate::tui::state::update::{ManagerMessage, action_of, update_manager};

use crate::tui::state::{
    AuthForm, AuthFormFocus, AuthFormTarget, CreatePreludeState, DragState, EditorState, EditorTab,
    FieldFocus, ManagerStage, ManagerState, MountScrollFocus, SettingsModal, SettingsState,
    SettingsTab,
};

use ratatui::layout::Rect;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
