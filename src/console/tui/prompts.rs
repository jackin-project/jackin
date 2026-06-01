//! Agent and role prompting helpers for the workspace manager event loop.

use crate::config::AppConfig;
use crate::console::ConsoleOutcome;
use crate::paths::JackinPaths;
use crate::selector::RoleSelector;
use crate::workspace::{LoadWorkspaceInput, ResolvedWorkspace};

use super::{ConsoleStage, ConsoleState};

pub(super) enum AgentPickerResolution {
    Opened,
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
    ms.status_overlay = Some(jackin_tui::components::StatusPopupState::new(
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn try_prompt_for_agent<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
) -> anyhow::Result<AgentPickerResolution>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    if workspace.default_agent.is_some() {
        return Ok(AgentPickerResolution::NotNeeded);
    }

    draw_role_resolution_dialog(terminal, state, config, cwd, role)?;
    let choices =
        match crate::console::services::agents::load_inline_picker_choices(
            paths, config, role, runner,
        )
        .await
        {
            Ok(Some(choices)) => choices,
            Ok(None) => return Ok(AgentPickerResolution::NotNeeded),
            Err(error) => return Ok(AgentPickerResolution::Failed(error)),
        };

    let ConsoleStage::Manager(ms) = &mut state.stage;
    ms.inline_agent_picker = Some((
        role.clone(),
        crate::agent::AgentChoiceState::with_choices(choices),
    ));
    ms.inline_role_picker = None;
    state.pending_launch_role = Some(role.clone());
    Ok(AgentPickerResolution::Opened)
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

#[allow(clippy::too_many_arguments)]
pub(in crate::console) async fn prompt_agent_for_launch<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    role: &RoleSelector,
    workspace: &ResolvedWorkspace,
    input: LoadWorkspaceInput,
    on_failure: OnPromptFailure,
) -> anyhow::Result<PromptOutcome>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    match try_prompt_for_agent(terminal, state, paths, config, cwd, runner, role, workspace).await?
    {
        AgentPickerResolution::Opened => {
            state.pending_launch = Some(input);
            Ok(PromptOutcome::Defer)
        }
        AgentPickerResolution::NotNeeded => Ok(PromptOutcome::Launch),
        AgentPickerResolution::Failed(error) => {
            if matches!(on_failure, OnPromptFailure::RestorePending) {
                state.pending_launch = Some(input);
            }
            show_role_resolution_error(state, role, &error);
            Ok(PromptOutcome::Defer)
        }
    }
}

pub(super) async fn dispatch_and_prompt_launch<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    input: LoadWorkspaceInput,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let Some((role, workspace, agent)) =
        crate::console::tui::launch::dispatch_launch_for_workspace(state, config, cwd, input.clone())?
    else {
        return Ok(None);
    };
    if agent.is_some() {
        return Ok(Some(ConsoleOutcome::Launch(role, workspace, agent)));
    }
    match prompt_agent_for_launch(
        terminal,
        state,
        paths,
        config,
        cwd,
        runner,
        &role,
        &workspace,
        input,
        OnPromptFailure::ClearPending,
    )
    .await?
    {
        PromptOutcome::Launch => Ok(Some(ConsoleOutcome::Launch(role, workspace, None))),
        PromptOutcome::Defer => Ok(None),
    }
}

pub(super) async fn prompt_committed_role<B>(
    terminal: &mut ratatui::Terminal<B>,
    state: &mut ConsoleState,
    paths: &JackinPaths,
    config: &AppConfig,
    cwd: &std::path::Path,
    runner: &mut impl crate::docker::CommandRunner,
    role: RoleSelector,
) -> anyhow::Result<Option<ConsoleOutcome>>
where
    B: ratatui::backend::Backend,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let Some(input) = state.pending_launch.take() else {
        return Ok(None);
    };
    let Some(resolved) =
        crate::console::domain::resolve_committed_role_launch(config, cwd, input, &role)?
    else {
        return Ok(None);
    };
    match prompt_agent_for_launch(
        terminal,
        state,
        paths,
        config,
        cwd,
        runner,
        &role,
        &resolved.workspace,
        resolved.input,
        OnPromptFailure::RestorePending,
    )
    .await?
    {
        PromptOutcome::Launch => {
            state.pending_launch_role = None;
            Ok(Some(ConsoleOutcome::Launch(role, resolved.workspace, None)))
        }
        PromptOutcome::Defer => Ok(None),
    }
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
