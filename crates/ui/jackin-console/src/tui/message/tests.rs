// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{
    AgentPickerChoices, AgentPickerResolution, ConsoleInstanceAction, MountInfoRefreshSourceFacts,
    MountInfoRefreshTarget, OnPromptFailure, PromptOutcome, agent_picker_choices_for_workspace,
    launch_agent_prompt_plan, launch_prompt_should_probe_agents, mount_info_refresh_source_plan,
};

use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

mod case_01;
