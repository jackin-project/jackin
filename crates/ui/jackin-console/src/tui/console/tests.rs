// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for console-level state helpers and prompt flows.

use super::*;

use crate::services::file_browser::listing_from_home;

use crate::tui::components::file_browser::FileBrowserState;

use crate::tui::components::{ConfirmState, TextInputState};

use crate::tui::console::{ConsoleStage, ConsoleState, new_console_state};

use crate::tui::debug::console_location_debug;

use crate::tui::debug::key_debug_name_for_input;

use crate::tui::message::{OnPromptFailure, PromptOutcome};

use crate::tui::model::{store_pending_launch_plan, take_pending_launch_plan};

use crate::tui::prompts::{
    ConcreteAgentPickerChoices as AgentPickerChoices, committed_role_prompt,
    launch_with_committed_agent, prompt_agent_for_launch, show_role_resolution_error,
};

use crate::tui::run::{consumes_letter_input, is_on_main_screen, letter_input_state_for_console};

use crate::tui::state::{
    EditorState, FileBrowserTarget, ManagerStage, Modal, SecretsScopeTag, TextInputTarget,
};

use jackin_config::{
    AppConfig, LoadWorkspaceInput, MountConfig, MountIsolation, ResolvedWorkspace, RoleSource,
    WorkspaceConfig,
};

use jackin_core::{Agent, RoleSelector};

use jackin_oppicker::ModalOutcome;

mod support;
use support::*;
mod case_01;
