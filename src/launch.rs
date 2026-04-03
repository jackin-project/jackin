use crate::config::{AppConfig, MountEntry};
use crate::selector::ClassSelector;
use crate::tui;
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
    crate::workspace::resolve_load_workspace(config, agent, cwd, choice.input.clone(), &[])
}

// ── Color palette (matching CLI banner) ────────────────────────────────

mod colors {
    use ratatui::style::Color;

    pub const BRIGHT_BLUE: Color = Color::Rgb(100, 149, 237); // circuit lines, labels
    pub const DIM_BLUE: Color = Color::Rgb(75, 105, 145); // borders, subtitle
    pub const DETAIL_BORDER: Color = Color::Rgb(55, 65, 85); // details panel border
    pub const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65); // highlight
    pub const DIM_GREEN: Color = Color::Rgb(0, 140, 30); // footer hints
    pub const WHITE: Color = Color::Rgb(255, 255, 255);
    pub const DIM_WHITE: Color = Color::Rgb(180, 180, 180);
    pub const PATH: Color = Color::Rgb(220, 190, 120); // paths (warm amber)
    pub const PATH_DST: Color = Color::Rgb(150, 180, 220); // mount destination
    pub const DARK_BG: Color = Color::Rgb(20, 20, 30); // subtle bg for selected
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
        terminal.draw(|frame| match state.stage {
            LaunchStage::Workspace => draw_workspace_screen(frame, &state),
            LaunchStage::Agent => draw_agent_screen(frame, &state),
        })?;
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match state.stage {
                LaunchStage::Workspace => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        state.selected_workspace = state.selected_workspace.saturating_sub(1);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
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

    drop(guard);
    result
}

// ── Full banner (matching CLI help colors) ──────────────────────────────

const BANNER_HEIGHT: u16 = 9;

fn render_banner(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let blue = Style::default().fg(colors::BRIGHT_BLUE);
    let title = Style::default().fg(colors::WHITE).add_modifier(Modifier::BOLD);
    let sub = Style::default().fg(colors::DIM_BLUE);

    // The logo is 25 chars wide ("│ │╷│ │╷│ ╷  │╷│ │╷│ │╷│").
    // Pre-pad each line to the same width so Alignment::Center keeps them grouped.
    let w = 25;
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("{:<w$}", "│ │╷│ │╷│ ╷  │╷│ │╷│ │╷│"), blue)),
        Line::from(Span::styled(format!("{:<w$}", "│ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│"), blue)),
        Line::from(Span::styled(format!("{:<w$}", "╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵"), blue)),
        Line::from(Span::styled(format!("{:<w$}", "           ╵"), blue)),
        Line::from(Span::styled(format!("{:^w$}", "j a c k i n"), title)),
        Line::from(Span::styled(format!("{:^w$}", "operator terminal"), sub)),
    ];

    // Center the logo block horizontally
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Length(w as u16),
            Constraint::Fill(1),
        ])
        .split(area);

    let banner = Paragraph::new(lines).alignment(Alignment::Left);
    frame.render_widget(banner, cols[1]);
}

// ── Screen 1: Workspace selection ──────────────────────────────────────

#[allow(clippy::too_many_lines)]
fn draw_workspace_screen(frame: &mut ratatui::Frame, state: &LaunchState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{
        Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap,
    };

    let area = frame.area();

    // Main vertical layout: banner | body | footer
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(BANNER_HEIGHT), // banner (includes bottom padding)
            Constraint::Min(10),              // body
            Constraint::Length(2),             // footer
        ])
        .split(area);

    // Banner
    render_banner(frame, root[0]);

    // Body: workspace list (top, 40%) + details (bottom, 60%) — fixed ratio
    let selected = &state.workspaces[state.selected_workspace];
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // workspace list
            Constraint::Percentage(60), // details (scrollable content in fixed area)
        ])
        .split(root[1]);

    // Center both panels at the same width
    let list_area = centered_rect(body[0], 70);
    let detail_area = centered_rect(body[1], 70);

    // Workspace list
    let workspace_items: Vec<ListItem> = state
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let label = if ws.name == "Current directory" {
                format!("  {}  (cwd)", tui::shorten_home(&ws.workspace.workdir))
            } else {
                format!("  {}  ", ws.name)
            };
            let style = if i == state.selected_workspace {
                Style::default()
                    .fg(colors::PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
                    .bg(colors::DARK_BG)
            } else {
                Style::default().fg(colors::DIM_WHITE)
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let ws_block = Block::default()
        .title(Span::styled(
            " Select Workspace ",
            Style::default()
                .fg(colors::BRIGHT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::DIM_BLUE));

    let workspace_list = List::new(workspace_items)
        .block(ws_block)
        .highlight_symbol("▸ ");
    let mut workspace_state = ListState::default();
    workspace_state.select(Some(state.selected_workspace));
    frame.render_stateful_widget(workspace_list, list_area, &mut workspace_state);

    // Details panel
    let mut detail_lines: Vec<Line> = Vec::new();

    detail_lines.push(Line::from(vec![
        Span::styled("workdir  ", Style::default().fg(colors::BRIGHT_BLUE)),
        Span::styled(
            tui::shorten_home(&selected.workspace.workdir),
            Style::default().fg(colors::PATH),
        ),
    ]));
    detail_lines.push(Line::from(vec![
        Span::styled("agents   ", Style::default().fg(colors::BRIGHT_BLUE)),
        Span::styled(
            format!("{} available", selected.allowed_agents.len()),
            Style::default().fg(colors::DIM_WHITE),
        ),
    ]));

    let all_mounts: Vec<(&MountConfig, bool)> = selected
        .workspace
        .mounts
        .iter()
        .map(|m| (m, false))
        .chain(selected.global_mounts.iter().map(|m| (m, true)))
        .collect();

    if !all_mounts.is_empty() {
        detail_lines.push(Line::from(""));
        for (mount, is_global) in &all_mounts {
            let src_short = tui::shorten_home(&mount.src);
            let dst_short = tui::shorten_home(&mount.dst);
            let ro = if mount.readonly { " (read-only)" } else { "" };
            let global_tag = if *is_global { " [global]" } else { "" };

            let mut spans = vec![Span::styled("  ", Style::default())];
            if src_short == dst_short {
                spans.push(Span::styled(src_short, Style::default().fg(colors::PATH)));
            } else {
                spans.push(Span::styled(
                    src_short,
                    Style::default().fg(colors::PATH),
                ));
                spans.push(Span::styled(
                    " mounted as ",
                    Style::default().fg(colors::DIM_WHITE),
                ));
                spans.push(Span::styled(
                    dst_short,
                    Style::default().fg(colors::PATH_DST),
                ));
            }
            if !ro.is_empty() || !global_tag.is_empty() {
                spans.push(Span::styled(
                    format!("{ro}{global_tag}"),
                    Style::default().fg(colors::DIM_WHITE),
                ));
            }
            detail_lines.push(Line::from(spans));
        }
    }

    let detail_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::DETAIL_BORDER));
    let details = Paragraph::new(detail_lines)
        .block(detail_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(details, detail_area);

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("  Enter ", Style::default().fg(colors::PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)),
        Span::styled("select   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled("↑↓ ", Style::default().fg(colors::PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)),
        Span::styled("navigate   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled("Esc ", Style::default().fg(colors::PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)),
        Span::styled("quit", Style::default().fg(colors::DIM_GREEN)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, root[2]);
}

// ── Screen 2: Agent selection ──────────────────────────────────────────

fn draw_agent_screen(frame: &mut ratatui::Frame, state: &LaunchState) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph};

    let area = frame.area();
    let selected_ws = &state.workspaces[state.selected_workspace];

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(BANNER_HEIGHT), // banner
            Constraint::Length(2),             // workspace context line
            Constraint::Min(8),               // agent list
            Constraint::Length(2),             // footer
        ])
        .split(area);

    // Banner
    render_banner(frame, root[0]);

    // Context: which workspace is selected
    let ws_label = if selected_ws.name == "Current directory" {
        tui::shorten_home(&selected_ws.workspace.workdir)
    } else {
        selected_ws.name.clone()
    };
    let context = Paragraph::new(Line::from(vec![
        Span::styled("  workspace: ", Style::default().fg(colors::DIM_BLUE)),
        Span::styled(ws_label, Style::default().fg(colors::WHITE).add_modifier(Modifier::BOLD)),
    ]));
    frame.render_widget(context, root[1]);

    // Agent list (centered)
    let list_area = centered_rect(root[2], 50);

    let agents = state.filtered_agents();
    let agent_items: Vec<ListItem> = agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let label = format!("  {}  ", agent.key());
            let style = if i == state.selected_agent {
                Style::default()
                    .fg(colors::PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
                    .bg(colors::DARK_BG)
            } else {
                Style::default().fg(colors::DIM_WHITE)
            };
            ListItem::new(Line::from(Span::styled(label, style)))
        })
        .collect();

    let title = if state.agent_query.is_empty() {
        " Select Agent ".to_string()
    } else {
        format!(" Select Agent (filter: {}) ", state.agent_query)
    };

    let agent_block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(colors::BRIGHT_BLUE)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::DIM_BLUE));

    let agent_list = List::new(agent_items)
        .block(agent_block)
        .highlight_symbol("▸ ");
    let mut agent_state = ListState::default();
    agent_state.select(Some(state.selected_agent));
    frame.render_stateful_widget(agent_list, list_area, &mut agent_state);

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("  Enter ", Style::default().fg(colors::PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)),
        Span::styled("load   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled("↑↓ ", Style::default().fg(colors::PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)),
        Span::styled("navigate   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled("Type ", Style::default().fg(colors::PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)),
        Span::styled("to filter   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled("Esc ", Style::default().fg(colors::PHOSPHOR_GREEN).add_modifier(Modifier::BOLD)),
        Span::styled("back", Style::default().fg(colors::DIM_GREEN)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, root[3]);
}

// ── Layout helpers ─────────────────────────────────────────────────────

/// Create a centered sub-rect within `area`, using `percent` of the width.
fn centered_rect(area: ratatui::layout::Rect, percent: u16) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    let side = (100_u16.saturating_sub(percent)) / 2;
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(side),
            Constraint::Percentage(percent),
            Constraint::Percentage(side),
        ])
        .split(area);
    cols[1]
}

#[cfg(test)]
const fn footer_text(stage: LaunchStage) -> &'static str {
    match stage {
        LaunchStage::Workspace => "Enter select   ↑↓ navigate   Esc quit",
        LaunchStage::Agent => "Enter load   ↑↓ navigate   Type to filter   Esc back",
    }
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
        assert!(footer_text(LaunchStage::Workspace).contains("Enter"));
        assert!(footer_text(LaunchStage::Workspace).contains("quit"));
        assert!(footer_text(LaunchStage::Agent).contains("Enter"));
        assert!(footer_text(LaunchStage::Agent).contains("back"));
        assert!(footer_text(LaunchStage::Agent).contains("filter"));
    }
}
