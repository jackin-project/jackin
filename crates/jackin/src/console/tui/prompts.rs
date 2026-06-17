//! Agent and role prompting helpers for the workspace manager event loop.

use crate::console::ConsoleOutcome;
use jackin_config::AppConfig;
use jackin_config::{LoadWorkspaceInput, ResolvedWorkspace};
use jackin_console::tui::components::error_popup::{
    role_resolution_error_message, role_resolution_error_title,
};
use jackin_console::tui::components::status_popup::role_resolution_status_popup_state;
use jackin_console::tui::message::AgentPickerResolution;
pub(in crate::console) use jackin_console::tui::message::{OnPromptFailure, PromptOutcome};
use jackin_core::RoleSelector;

use super::{ConsoleStage, ConsoleState};

pub(in crate::console) type AgentPickerChoices =
    jackin_console::tui::message::AgentPickerChoices<jackin_core::Agent>;

pub(super) fn draw_role_resolution_dialog<B>(
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
    ms.status_overlay = Some(role_resolution_status_popup_state(role.key()));
    terminal.draw(|frame| {
        crate::console::tui::render(frame, frame.area(), ms, config, cwd);
    })?;
    ms.status_overlay = None;
    Ok(())
}

pub(in crate::console) fn show_role_resolution_error(
    state: &mut ConsoleState,
    role: &RoleSelector,
    error: &anyhow::Error,
) {
    let ConsoleStage::Manager(ms) = &mut state.stage;
    let _unused = crate::console::tui::update_manager(
        ms,
        crate::console::tui::ManagerMessage::OpenListErrorPopup {
            title: role_resolution_error_title().into(),
            message: role_resolution_error_message(role.key(), error),
        },
    );
}

pub(super) fn try_prompt_for_agent(
    state: &mut ConsoleState,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    choices: AgentPickerChoices,
) -> AgentPickerResolution {
    if workspace.default_agent.is_some() {
        return AgentPickerResolution::NotNeeded;
    }

    let choices = match choices {
        AgentPickerChoices::Choices(choices) => choices,
        AgentPickerChoices::NotNeeded => return AgentPickerResolution::NotNeeded,
        AgentPickerChoices::Failed(error) => return AgentPickerResolution::Failed(error),
    };

    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.inline_agent_picker = Some((
        role.clone(),
        jackin_console::tui::components::agent_choice::AgentChoiceState::with_choices(choices),
    ));
    ms.inline_role_picker = None;
    state.pending_launch_role = Some(role.clone());
    AgentPickerResolution::Opened
}

pub(in crate::console) fn prompt_agent_for_launch(
    state: &mut ConsoleState,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    input: LoadWorkspaceInput,
    on_failure: OnPromptFailure,
    choices: AgentPickerChoices,
) -> PromptOutcome {
    match try_prompt_for_agent(state, role, workspace, choices) {
        AgentPickerResolution::Opened => {
            state.pending_launch = Some(input);
            PromptOutcome::Defer
        }
        AgentPickerResolution::NotNeeded => PromptOutcome::Launch,
        AgentPickerResolution::Failed(error) => {
            if matches!(on_failure, OnPromptFailure::RestorePending) {
                state.pending_launch = Some(input);
            }
            show_role_resolution_error(state, role, &error);
            PromptOutcome::Defer
        }
    }
}

pub(super) fn dispatch_launch_prompt(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    input: LoadWorkspaceInput,
) -> anyhow::Result<LaunchPromptDispatch> {
    let Some((role, workspace, agent)) =
        crate::console::tui::launch::dispatch_launch_for_workspace(
            state,
            config,
            cwd,
            input.clone(),
        )?
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

pub(super) fn committed_role_prompt(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    role: RoleSelector,
) -> anyhow::Result<LaunchPromptDispatch> {
    let Some(input) = state.pending_launch.take() else {
        return Ok(LaunchPromptDispatch::None);
    };
    let Some(resolved) =
        jackin_console::services::launch::resolve_committed_role_launch(config, cwd, input, &role)?
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

pub(super) type LaunchPromptDispatch =
    jackin_console::tui::message::LaunchPromptDispatch<ConsoleOutcome, LaunchPromptRequest>;

pub(super) type LaunchPromptRequest = jackin_console::tui::message::LaunchPromptRequest<
    RoleSelector,
    ResolvedWorkspace,
    LoadWorkspaceInput,
>;

pub(super) fn launch_with_committed_agent(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    agent: jackin_core::Agent,
) -> anyhow::Result<Option<ConsoleOutcome>> {
    let (Some(input), Some(role)) = (
        state.pending_launch.take(),
        state.pending_launch_role.take(),
    ) else {
        return Ok(None);
    };
    let Some(resolved) =
        crate::console::domain::resolve_committed_agent_launch(config, cwd, input, role, agent)?
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

    if let ConsoleStage::Manager(ms) = &mut state.stage {
        ms.launch_provider_picker = Some(crate::console::tui::state::ProviderPickerState::new(
            resolved.role.clone(),
            agent,
            resolved.providers,
        ));
    }
    state.pending_launch = Some(resolved.input);
    state.pending_launch_role = Some(resolved.role);
    Ok(None)
}
