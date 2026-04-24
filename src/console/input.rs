use super::preview::resolve_selected_workspace;
use super::state::{ConsoleStage, ConsoleState};
use crate::config::AppConfig;
use crate::selector::ClassSelector;
use crate::workspace::ResolvedWorkspace;

pub(super) enum EventOutcome {
    Continue,
    Exit(anyhow::Result<Option<(ClassSelector, ResolvedWorkspace)>>),
}

pub(super) fn handle_event(
    state: &mut ConsoleState,
    key: crossterm::event::KeyCode,
    config: &AppConfig,
    cwd: &std::path::Path,
) -> EventOutcome {
    use crossterm::event::KeyCode;
    match &state.stage {
        ConsoleStage::Agent => match key {
            KeyCode::Esc => {
                // Return to the manager list.
                state.stage = ConsoleStage::Manager(
                    crate::console::manager::ManagerState::from_config(config, cwd),
                );
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
        ConsoleStage::Manager(_) => {
            // Manager stage is handled directly in run_console; this branch
            // should never be reached via handle_event.
        }
    }
    EventOutcome::Continue
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_esc_returns_to_manager() {
        let temp = tempfile::tempdir().unwrap();
        let config = AppConfig::default();
        // Construct a state in Agent stage to test Esc.
        let mut state = ConsoleState::new(&config, temp.path()).unwrap();
        state.stage = ConsoleStage::Agent;

        let outcome = handle_event(
            &mut state,
            crossterm::event::KeyCode::Esc,
            &config,
            temp.path(),
        );

        assert!(
            matches!(outcome, EventOutcome::Continue),
            "Esc from Agent should return Continue (not Exit)"
        );
        assert!(
            matches!(state.stage, ConsoleStage::Manager(_)),
            "Esc from Agent should transition stage to Manager"
        );
    }
}
