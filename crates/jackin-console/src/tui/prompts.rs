// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Agent and role prompting helpers for the workspace manager event loop.

use jackin_config::{AppConfig, LoadWorkspaceInput, ResolvedWorkspace};
use jackin_core::RoleSelector;

use crate::tui::components::error_popup::{
    role_resolution_error_message, role_resolution_error_title,
};
use crate::tui::console::{ConsoleOutcome, ConsoleStage, ConsoleState};
pub use crate::tui::message::{AgentPickerChoices, LaunchPromptDispatch, LaunchPromptRequest};
use crate::tui::message::{
    AgentPickerResolution, OnPromptFailure, PromptOutcome, agent_picker_choices_for_workspace,
    launch_agent_prompt_plan,
};
use crate::tui::model::{
    open_launch_agent_prompt_plan, open_launch_provider_picker_plan, store_pending_launch_plan,
    take_pending_launch_and_role_plan, take_pending_launch_plan,
};
use crate::tui::state::update::{ManagerMessage, update_manager};
use crate::tui::update::{
    apply_status_overlay_plan, dismiss_status_overlay_plan, role_resolution_status_overlay_plan,
};

pub type ConcreteAgentPickerChoices = AgentPickerChoices<jackin_core::Agent>;

pub type ConcreteLaunchPromptDispatch =
    LaunchPromptDispatch<ConsoleOutcome, ConcreteLaunchPromptRequest>;

pub type ConcreteLaunchPromptRequest =
    LaunchPromptRequest<RoleSelector, ResolvedWorkspace, LoadWorkspaceInput>;

pub fn draw_role_resolution_dialog<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    role: &RoleSelector,
) -> anyhow::Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let ConsoleStage::Manager(ms) = &mut state.stage;
    apply_status_overlay_plan(ms, role_resolution_status_overlay_plan(role.key()));
    terminal.draw(|frame| {
        crate::tui::view::render(frame, frame.area(), ms, config, cwd);
    })?;
    apply_status_overlay_plan(ms, dismiss_status_overlay_plan());
    Ok(())
}

pub fn show_role_resolution_error(
    state: &mut ConsoleState,
    role: &RoleSelector,
    error: &anyhow::Error,
) {
    let ConsoleStage::Manager(ms) = &mut state.stage;
    let _unused = update_manager(
        ms,
        ManagerMessage::OpenListErrorPopup {
            title: role_resolution_error_title().into(),
            message: role_resolution_error_message(role.key(), error),
        },
    );
}

fn try_prompt_for_agent(
    state: &mut ConsoleState,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    choices: ConcreteAgentPickerChoices,
) -> AgentPickerResolution {
    let choices =
        match agent_picker_choices_for_workspace(workspace.default_agent.is_some(), choices) {
            AgentPickerChoices::Choices(choices) => choices,
            AgentPickerChoices::NotNeeded => return AgentPickerResolution::NotNeeded,
            AgentPickerChoices::Failed(error) => return AgentPickerResolution::Failed(error),
        };

    open_launch_agent_prompt_plan(state, role.clone(), choices);
    AgentPickerResolution::Opened
}

pub fn prompt_agent_for_launch(
    state: &mut ConsoleState,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    input: LoadWorkspaceInput,
    on_failure: OnPromptFailure,
    choices: ConcreteAgentPickerChoices,
) -> PromptOutcome {
    let plan = launch_agent_prompt_plan(
        try_prompt_for_agent(state, role, workspace, choices),
        on_failure,
    );
    if plan.store_pending_launch {
        store_pending_launch_plan(state, input);
    }
    if let Some(error) = plan.error {
        show_role_resolution_error(state, role, &error);
    }
    plan.outcome
}

pub fn dispatch_launch_prompt(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<ConcreteLaunchPromptDispatch> {
    let Some((role, workspace, agent)) =
        crate::tui::launch::dispatch_launch_for_workspace(state, config, cwd, input.clone())?
    else {
        return Ok(LaunchPromptDispatch::None);
    };
    if agent.is_some() {
        return Ok(LaunchPromptDispatch::Launch(ConsoleOutcome::Launch(
            role, workspace, agent,
        )));
    }
    Ok(LaunchPromptDispatch::Prompt(LaunchPromptRequest {
        role,
        workspace,
        input,
        on_failure: OnPromptFailure::ClearPending,
    }))
}

pub fn committed_role_prompt(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    role: RoleSelector,
) -> anyhow::Result<ConcreteLaunchPromptDispatch> {
    let Some(input) = take_pending_launch_plan(state) else {
        return Ok(LaunchPromptDispatch::None);
    };
    let Some(resolved) =
        crate::services::launch::resolve_committed_role_launch(config, cwd, input, &role)?
    else {
        return Ok(LaunchPromptDispatch::None);
    };
    Ok(LaunchPromptDispatch::Prompt(LaunchPromptRequest {
        role,
        workspace: resolved.workspace,
        input: resolved.input,
        on_failure: OnPromptFailure::RestorePending,
    }))
}

pub fn launch_with_committed_agent(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    agent: jackin_core::Agent,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    let Some((input, role)) = take_pending_launch_and_role_plan(state) else {
        return Ok(None);
    };
    let Some(resolved) =
        crate::services::launch::resolve_committed_agent_launch(config, cwd, input, role, agent)?
    else {
        return Ok(None);
    };
    if resolved.providers.is_empty() {
        return Ok(Some(ConsoleOutcome::Launch(
            resolved.role,
            resolved.workspace,
            Some(agent),
        )));
    }

    open_launch_provider_picker_plan(
        state,
        resolved.input,
        resolved.role,
        agent,
        resolved.providers,
    );
    Ok(None)
}
