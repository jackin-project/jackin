use super::preview::resolve_selected_workspace;
use super::state::{LaunchStage, LaunchState};
use crate::app::context::preferred_agent_index;
use crate::config::AppConfig;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

pub(super) enum EventOutcome {
    Continue,
    Exit(anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>>),
}

#[allow(clippy::too_many_lines)]
pub(super) fn handle_event(
    state: &mut LaunchState,
    key: crossterm::event::KeyCode,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> EventOutcome {
    use crossterm::event::KeyCode;
    match &state.stage {
        LaunchStage::Workspace => match key {
            KeyCode::Up | KeyCode::Char('k') => {
                state.selected_workspace = state.selected_workspace.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.selected_workspace =
                    (state.selected_workspace + 1).min(state.workspaces.len().saturating_sub(1));
            }
            KeyCode::Char('m') => {
                if std::env::var("JACKIN_NO_ANIMATIONS").ok().as_deref() != Some("1") {
                    crate::tui::animation::digital_rain(400, None);
                }
                state.stage =
                    LaunchStage::Manager(crate::launch::manager::ManagerState::from_config(config));
            }
            KeyCode::Enter => {
                let agents = state.filtered_agents();
                if agents.is_empty() {
                    return EventOutcome::Exit(Err(anyhow::anyhow!(
                        "no eligible agents for workspace {}",
                        state.workspaces[state.selected_workspace].name
                    )));
                }
                if agents.len() == 1 {
                    let agent = agents[0].clone();
                    let workspace = match resolve_selected_workspace(
                        config,
                        cwd,
                        &state.workspaces[state.selected_workspace],
                        &agent,
                    ) {
                        Ok(v) => v,
                        Err(e) => return EventOutcome::Exit(Err(e)),
                    };
                    return EventOutcome::Exit(Ok(Some((agent, workspace))));
                }
                state.stage = LaunchStage::Agent;
                state.agent_query.clear();
                let choice = &state.workspaces[state.selected_workspace];
                state.selected_agent = preferred_agent_index(
                    &agents,
                    choice.last_agent.as_deref(),
                    choice.default_agent.as_deref(),
                )
                .unwrap_or(0);
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                return EventOutcome::Exit(Ok(None));
            }
            _ => {}
        },
        LaunchStage::Agent => match key {
            KeyCode::Esc => {
                state.stage = LaunchStage::Workspace;
                state.agent_query.clear();
                state.selected_agent = 0;
            }
            KeyCode::Backspace => {
                state.agent_query.pop();
                state.selected_agent = 0;
            }
            KeyCode::Char(ch) => {
                state.agent_query.push(ch);
                state.selected_agent = 0;
            }
            KeyCode::Up => {
                state.selected_agent = state.selected_agent.saturating_sub(1);
            }
            KeyCode::Down => {
                state.selected_agent =
                    (state.selected_agent + 1).min(state.filtered_agents().len().saturating_sub(1));
            }
            KeyCode::Enter => {
                let agents = state.filtered_agents();
                let agent = match agents
                    .get(state.selected_agent)
                    .ok_or_else(|| anyhow::anyhow!("no agent selected"))
                {
                    Ok(v) => v,
                    Err(e) => return EventOutcome::Exit(Err(e)),
                };
                let workspace = match resolve_selected_workspace(
                    config,
                    cwd,
                    &state.workspaces[state.selected_workspace],
                    agent,
                ) {
                    Ok(v) => v,
                    Err(e) => return EventOutcome::Exit(Err(e)),
                };
                return EventOutcome::Exit(Ok(Some((agent.clone(), workspace))));
            }
            _ => {}
        },
        LaunchStage::Manager(_) => {
            // Manager stage is handled directly in run_launch; this branch
            // should never be reached via handle_event.
        }
    }
    EventOutcome::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_q_exits_without_error() {
        let temp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        let mut state = LaunchState::new(&config, temp.path()).unwrap();

        let outcome = handle_event(
            &mut state,
            crossterm::event::KeyCode::Char('q'),
            &config,
            temp.path(),
        );

        assert!(matches!(outcome, EventOutcome::Exit(Ok(None))));
    }
}
