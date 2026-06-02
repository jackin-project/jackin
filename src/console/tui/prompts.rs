//! Agent and role prompting helpers for the workspace manager event loop.

use crate::config::AppConfig;
use crate::console::ConsoleOutcome;
use crate::selector::RoleSelector;
use crate::workspace::{LoadWorkspaceInput, ResolvedWorkspace};

use super::{ConsoleStage, ConsoleState};

pub(super) enum AgentPickerResolution {
    Opened,
    NotNeeded,
    Failed(anyhow::Error),
}

pub(in crate::console) enum AgentPickerChoices {
    Choices(Vec<crate::agent::Agent>),
    NotNeeded,
    Failed(anyhow::Error),
}

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
    ms.status_overlay = Some(jackin_console::tui::components::status_popup::status_popup_state(
        "Resolving agent role",
        format!("Loading and resolving {}", role.key()),
    ));
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
    let _ = crate::console::tui::update_manager(
        ms,
        crate::console::tui::ManagerMessage::OpenListErrorPopup {
            title: "Role resolution failed".into(),
            message: format!("Could not resolve {}.\n\n{error:#}", role.key()),
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
        crate::agent::AgentChoiceState::with_choices(choices),
    ));
    ms.inline_role_picker = None;
    state.pending_launch_role = Some(role.clone());
    AgentPickerResolution::Opened
}

/// Outcome of `prompt_agent_for_launch`. Two states because callers
/// only branch on "the helper already drives the next interaction"
/// (`Defer`) vs "no prompt was needed, launch immediately" (`Launch`).
pub(in crate::console) enum PromptOutcome {
    Launch,
    Defer,
}

/// Whether `prompt_agent_for_launch` should hold the pending-launch
/// pin so the operator can retry after dismissing the error popup.
/// Arms that pinned `pending_launch` upstream pass `RestorePending`;
/// arms that built `input` fresh from the key event pass `ClearPending`.
#[derive(Clone, Copy)]
pub(in crate::console) enum OnPromptFailure {
    ClearPending,
    RestorePending,
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
        crate::console::tui::launch::dispatch_launch_for_workspace(state, config, cwd, input.clone())?
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
        crate::console::domain::resolve_committed_role_launch(config, cwd, input, &role)?
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

pub(super) enum LaunchPromptDispatch {
    Launch(ConsoleOutcome),
    Prompt(LaunchPromptRequest),
    None,
}

pub(super) struct LaunchPromptRequest {
    pub(super) role: RoleSelector,
    pub(super) workspace: ResolvedWorkspace,
    pub(super) input: LoadWorkspaceInput,
    pub(super) on_failure: OnPromptFailure,
}

pub(super) fn launch_with_committed_agent(
    state: &mut ConsoleState,
    config: &AppConfig,
    cwd: &std::path::Path,
    agent: crate::agent::Agent,
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
