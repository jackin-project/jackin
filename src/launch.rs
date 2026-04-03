use crate::config::{AppConfig, MountEntry};
use crate::selector::ClassSelector;
use crate::workspace::{LoadWorkspaceInput, MountConfig, ResolvedWorkspace, current_dir_workspace};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStage {
    Workspace,
    Agent,
}

#[derive(Debug, Clone)]
pub struct WorkspaceChoice {
    pub name: String,
    pub workspace: ResolvedWorkspace,
    pub allowed_agents: Vec<ClassSelector>,
    pub default_agent: Option<String>,
    pub global_mounts: Vec<MountConfig>,
    pub input: LoadWorkspaceInput,
}

#[derive(Debug, Clone)]
pub struct LaunchState {
    pub stage: LaunchStage,
    pub selected_workspace: usize,
    pub selected_agent: usize,
    pub agent_query: String,
    pub workspaces: Vec<WorkspaceChoice>,
}

impl LaunchState {
    pub fn new(config: &AppConfig, cwd: &std::path::Path) -> anyhow::Result<Self> {
        let current = current_dir_workspace(cwd)?;
        let global_mounts = global_mounts(config)?;
        let current_choice = WorkspaceChoice {
            name: "Current directory".to_string(),
            workspace: ResolvedWorkspace {
                label: current.workdir.clone(),
                workdir: current.workdir,
                mounts: current.mounts,
            },
            allowed_agents: configured_agents(config),
            default_agent: None,
            global_mounts: global_mounts.clone(),
            input: LoadWorkspaceInput::CurrentDir,
        };

        let mut workspaces = vec![current_choice];
        for (name, saved) in &config.workspaces {
            let allowed_agents = eligible_agents_for_saved_workspace(config, saved);
            workspaces.push(WorkspaceChoice {
                name: name.clone(),
                workspace: ResolvedWorkspace {
                    label: name.clone(),
                    workdir: saved.workdir.clone(),
                    mounts: saved.mounts.clone(),
                },
                allowed_agents,
                default_agent: saved.default_agent.clone(),
                global_mounts: global_mounts.clone(),
                input: LoadWorkspaceInput::Saved(name.clone()),
            });
        }

        let selected_workspace = workspaces
            .iter()
            .position(|choice| {
                choice.name != "Current directory"
                    && choice.workspace.workdir == cwd.display().to_string()
            })
            .unwrap_or(0);

        Ok(Self {
            stage: LaunchStage::Workspace,
            selected_workspace,
            selected_agent: 0,
            agent_query: String::new(),
            workspaces,
        })
    }

    pub fn selected_workspace_name(&self) -> Option<&str> {
        self.workspaces
            .get(self.selected_workspace)
            .map(|choice| choice.name.as_str())
    }

    pub fn filtered_agents(&self) -> Vec<ClassSelector> {
        let query = self.agent_query.to_ascii_lowercase();
        self.workspaces[self.selected_workspace]
            .allowed_agents
            .iter()
            .filter(|agent| query.is_empty() || agent.key().to_ascii_lowercase().contains(&query))
            .cloned()
            .collect()
    }
}

fn configured_agents(config: &AppConfig) -> Vec<ClassSelector> {
    config
        .agents
        .keys()
        .filter_map(|key| ClassSelector::parse(key).ok())
        .collect()
}

fn eligible_agents_for_saved_workspace(
    config: &AppConfig,
    workspace: &crate::workspace::WorkspaceConfig,
) -> Vec<ClassSelector> {
    configured_agents(config)
        .into_iter()
        .filter(|agent| {
            workspace.allowed_agents.is_empty()
                || workspace
                    .allowed_agents
                    .iter()
                    .any(|allowed| allowed == &agent.key())
        })
        .collect()
}

fn global_mounts(config: &AppConfig) -> anyhow::Result<Vec<MountConfig>> {
    let mounts = config
        .docker
        .mounts
        .iter()
        .filter_map(|(name, entry)| match entry {
            MountEntry::Mount(mount) => Some((name.clone(), mount.clone())),
            MountEntry::Scoped(_) => None,
        })
        .collect::<Vec<_>>();

    AppConfig::expand_and_validate_named_mounts(&mounts)
}

fn default_agent_index(choice: &WorkspaceChoice, agents: &[ClassSelector]) -> Option<usize> {
    choice
        .default_agent
        .as_ref()
        .and_then(|default| agents.iter().position(|agent| agent.key() == *default))
}

fn resolve_selected_workspace(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    agent: &ClassSelector,
) -> anyhow::Result<ResolvedWorkspace> {
    crate::workspace::resolve_load_workspace(config, agent, cwd, choice.input.clone())
}

#[allow(clippy::too_many_lines)]
pub fn run_launch(
    config: &AppConfig,
    cwd: &std::path::Path,
) -> anyhow::Result<(ClassSelector, ResolvedWorkspace)> {
    use crossterm::ExecutableCommand;
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{EnterAlternateScreen, enable_raw_mode};

    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = crossterm::terminal::disable_raw_mode();
            let mut stdout = std::io::stdout();
            let _ = stdout.execute(crossterm::terminal::LeaveAlternateScreen);
            let _ = stdout.execute(crossterm::cursor::Show);
        }
    }

    let mut state = LaunchState::new(config, cwd)?;
    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    let guard = TerminalGuard;
    stdout.execute(EnterAlternateScreen)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    let result = loop {
        terminal.draw(|frame| draw_launch(frame, &state))?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match state.stage {
                LaunchStage::Workspace => match key.code {
                    KeyCode::Up => {
                        state.selected_workspace = state.selected_workspace.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        state.selected_workspace = (state.selected_workspace + 1)
                            .min(state.workspaces.len().saturating_sub(1));
                    }
                    KeyCode::Enter => {
                        let agents = state.filtered_agents();
                        if agents.is_empty() {
                            break Err(anyhow::anyhow!(
                                "no eligible agents for workspace {}",
                                state.workspaces[state.selected_workspace].name
                            ));
                        }
                        if agents.len() == 1 {
                            let agent = agents[0].clone();
                            let workspace = resolve_selected_workspace(
                                config,
                                cwd,
                                &state.workspaces[state.selected_workspace],
                                &agent,
                            )?;
                            break Ok((agent, workspace));
                        }
                        state.stage = LaunchStage::Agent;
                        state.agent_query.clear();
                        state.selected_agent = default_agent_index(
                            &state.workspaces[state.selected_workspace],
                            &agents,
                        )
                        .unwrap_or(0);
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        break Err(anyhow::anyhow!("launch cancelled"));
                    }
                    _ => {}
                },
                LaunchStage::Agent => match key.code {
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
                        state.selected_agent = (state.selected_agent + 1)
                            .min(state.filtered_agents().len().saturating_sub(1));
                    }
                    KeyCode::Enter => {
                        let agents = state.filtered_agents();
                        let agent = agents
                            .get(state.selected_agent)
                            .ok_or_else(|| anyhow::anyhow!("no agent selected"))?;
                        let workspace = resolve_selected_workspace(
                            config,
                            cwd,
                            &state.workspaces[state.selected_workspace],
                            agent,
                        )?;
                        break Ok((agent.clone(), workspace));
                    }
                    _ => {}
                },
            }
        }
    };

    // TerminalGuard handles cleanup (disable_raw_mode, LeaveAlternateScreen, show cursor) on drop
    drop(guard);
    result
}

const fn footer_text(stage: LaunchStage) -> &'static str {
    match stage {
        LaunchStage::Workspace => "Enter select   Esc/q quit",
        LaunchStage::Agent => "Enter load   Esc back   Type to filter",
    }
}

fn draw_launch(frame: &mut ratatui::Frame, state: &LaunchState) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(frame.area());
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(root[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Min(8)])
        .split(body[1]);

    let workspace_items = state
        .workspaces
        .iter()
        .map(|workspace| ListItem::new(workspace.name.clone()))
        .collect::<Vec<_>>();
    let workspace_list = List::new(workspace_items)
        .block(Block::default().title("Workspaces").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut workspace_state = ListState::default();
    workspace_state.select(Some(state.selected_workspace));
    frame.render_stateful_widget(workspace_list, body[0], &mut workspace_state);

    let selected_workspace = &state.workspaces[state.selected_workspace];
    let mount_lines = selected_workspace
        .workspace
        .mounts
        .iter()
        .map(|mount| {
            let ro = if mount.readonly { " (ro)" } else { "" };
            format!("{} -> {}{}", mount.src, mount.dst, ro)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let global_lines = selected_workspace
        .global_mounts
        .iter()
        .map(|mount| {
            let ro = if mount.readonly { " (ro)" } else { "" };
            format!("{} -> {}{}", mount.src, mount.dst, ro)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let details = Paragraph::new(format!(
        "available agents: {}\nworkdir: {}\n\nmounts:\n{}\n\nglobal:\n{}",
        selected_workspace.allowed_agents.len(),
        selected_workspace.workspace.workdir,
        mount_lines,
        global_lines,
    ))
    .block(
        Block::default()
            .title("Workspace Details")
            .borders(Borders::ALL),
    );
    frame.render_widget(details, right[0]);

    let agent_items = state
        .filtered_agents()
        .into_iter()
        .map(|agent| ListItem::new(agent.key()))
        .collect::<Vec<_>>();
    let agent_title = if state.stage == LaunchStage::Agent {
        format!("Agents (filter: {})", state.agent_query)
    } else {
        "Agents".to_string()
    };
    let agent_list = List::new(agent_items)
        .block(Block::default().title(agent_title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::BOLD));
    let mut agent_state = ListState::default();
    agent_state.select(Some(state.selected_agent));
    frame.render_stateful_widget(agent_list, right[1], &mut agent_state);

    let footer =
        Paragraph::new(footer_text(state.stage)).block(Block::default().borders(Borders::TOP));
    frame.render_widget(footer, root[2]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preselects_saved_workspace_on_exact_workdir_match() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().canonicalize().unwrap();
        let workdir = project_dir.display().to_string();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "git@github.com:donbeave/jackin-agent-smith.git".to_string(),
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: workdir.clone(),
                mounts: vec![crate::workspace::MountConfig {
                    src: workdir.clone(),
                    dst: workdir,
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
            },
        );

        let state = LaunchState::new(&config, &project_dir).unwrap();
        assert_eq!(state.selected_workspace_name(), Some("big-monorepo"));
    }

    #[test]
    fn filters_agents_by_query() {
        let state = LaunchState {
            stage: LaunchStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: "chainargos".to_string(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![
                    crate::selector::ClassSelector::new(None, "agent-smith"),
                    crate::selector::ClassSelector::new(Some("chainargos"), "the-architect"),
                ],
                default_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        let filtered = state.filtered_agents();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key(), "chainargos/the-architect");
    }

    #[test]
    fn footer_text_matches_stage_behavior() {
        assert_eq!(
            footer_text(LaunchStage::Workspace),
            "Enter select   Esc/q quit"
        );
        assert_eq!(
            footer_text(LaunchStage::Agent),
            "Enter load   Esc back   Type to filter"
        );
    }
}
