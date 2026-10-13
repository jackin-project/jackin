// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.

use super::*;

use crate::tui::components::error_popup::{
    no_instance_state_for_workspace_message, no_purgeable_instance_for_workspace_message,
    no_recoverable_instance_for_workspace_message, no_running_instance_for_workspace_message,
    no_running_instance_to_stop_message,
};

use crate::tui::components::github_picker::GithubOpenPlan;

use crate::tui::focus::MountScrollFocus;

use jackin_config::{MountConfig, WorkspaceConfig};

use ratatui::layout::Rect;

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
