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
    pub last_agent: Option<String>,
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
            last_agent: None,
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
                last_agent: saved.last_agent.clone(),
                global_mounts: global_mounts.clone(),
                input: LoadWorkspaceInput::Saved(name.clone()),
            });
        }

        let selected_workspace = workspaces
            .iter()
            .enumerate()
            .filter_map(|(index, choice)| {
                if choice.name == "Current directory" {
                    return None;
                }

                match &choice.input {
                    LoadWorkspaceInput::Saved(name) => config
                        .workspaces
                        .get(name)
                        .and_then(|workspace| {
                            crate::workspace::saved_workspace_match_depth(workspace, cwd)
                        })
                        .map(|depth| (index, depth)),
                    _ => None,
                }
            })
            .max_by_key(|(_, depth)| *depth)
            .map_or(0, |(index, _)| index);

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
    // Last-used agent takes priority, then falls back to default_agent
    choice
        .last_agent
        .as_ref()
        .and_then(|last| agents.iter().position(|agent| agent.key() == *last))
        .or_else(|| {
            choice
                .default_agent
                .as_ref()
                .and_then(|default| agents.iter().position(|agent| agent.key() == *default))
        })
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
    pub const DETAIL_BORDER: Color = Color::Rgb(60, 75, 90); // details panel border
    pub const DETAIL_BG: Color = Color::Rgb(15, 17, 25); // details panel background
    pub const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65); // highlight
    pub const DIM_GREEN: Color = Color::Rgb(0, 140, 30); // footer hints
    pub const WHITE: Color = Color::Rgb(255, 255, 255);
    pub const DIM_WHITE: Color = Color::Rgb(180, 180, 180);
    pub const TAG: Color = Color::Rgb(120, 120, 140); // dim tags like "current directory"
    pub const PATH: Color = Color::Rgb(220, 190, 120); // paths (warm amber)
    pub const PATH_DST: Color = Color::Rgb(150, 180, 220); // mount destination
    pub const DARK_BG: Color = Color::Rgb(20, 20, 30); // subtle bg for selected
    pub const ERROR: Color = Color::Rgb(230, 120, 120);
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
            LaunchStage::Agent => draw_agent_screen(frame, &state, config, cwd),
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
    let title = Style::default()
        .fg(colors::WHITE)
        .add_modifier(Modifier::BOLD);
    let sub = Style::default().fg(colors::DIM_BLUE);

    // The logo is 25 chars wide ("│ │╷│ │╷│ ╷  │╷│ │╷│ │╷│").
    // Pre-pad each line to the same width so Alignment::Center keeps them grouped.
    let w = 25;
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("{:<w$}", "│ │╷│ │╷│ ╷  │╷│ │╷│ │╷│"),
            blue,
        )),
        Line::from(Span::styled(
            format!("{:<w$}", "│ ╵│ │╵│ ╵ ╷ ╵│ │╵│ │╵│"),
            blue,
        )),
        Line::from(Span::styled(
            format!("{:<w$}", "╵  ╵ ╵ ╵  │  ╵ ╵ ╵ ╵ ╵"),
            blue,
        )),
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
            Constraint::Min(10),               // body
            Constraint::Length(2),             // footer
        ])
        .split(area);

    // Banner
    render_banner(frame, root[0]);

    // Body: workspace list (fixed height) + gap + details (fills rest)
    let selected = &state.workspaces[state.selected_workspace];
    let list_height = (state.workspaces.len() as u16) + 2; // items + border top/bottom
    let body = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(list_height), // workspace list — fixed to item count
            Constraint::Length(1),           // gap between panels
            Constraint::Min(6),              // details — fills remaining space
        ])
        .split(root[1]);

    // Center both panels at the same width
    let list_area = centered_rect(body[0], 70);
    let detail_area = centered_rect(body[2], 70);

    // Workspace list
    let workspace_items: Vec<ListItem> = state
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, ws)| {
            let is_selected = i == state.selected_workspace;
            let name_style = if is_selected {
                Style::default()
                    .fg(colors::PHOSPHOR_GREEN)
                    .add_modifier(Modifier::BOLD)
                    .bg(colors::DARK_BG)
            } else {
                Style::default().fg(colors::DIM_WHITE)
            };

            if ws.name == "Current directory" {
                let path_style = if is_selected {
                    Style::default()
                        .fg(colors::WHITE)
                        .add_modifier(Modifier::BOLD)
                        .bg(colors::DARK_BG)
                } else {
                    Style::default().fg(colors::WHITE)
                };
                let tag_style = if is_selected {
                    Style::default().fg(colors::TAG).bg(colors::DARK_BG)
                } else {
                    Style::default().fg(colors::TAG)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("  {}  ", tui::shorten_home(&ws.workspace.workdir)),
                        path_style,
                    ),
                    Span::styled("current directory", tag_style),
                ]))
            } else {
                ListItem::new(Line::from(Span::styled(
                    format!("  {}  ", ws.name),
                    name_style,
                )))
            }
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
            Style::default().fg(colors::WHITE),
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
        detail_lines.push(Line::from(Span::styled(
            "mounts",
            Style::default()
                .fg(colors::BRIGHT_BLUE)
                .add_modifier(Modifier::BOLD),
        )));
        for (mount, is_global) in &all_mounts {
            let src_short = tui::shorten_home(&mount.src);
            let dst_short = tui::shorten_home(&mount.dst);
            let ro = if mount.readonly { " (read-only)" } else { "" };
            let global_tag = if *is_global { " [global]" } else { "" };

            let mut spans = vec![Span::styled("  ", Style::default())];
            if src_short == dst_short {
                spans.push(Span::styled(src_short, Style::default().fg(colors::PATH)));
            } else {
                spans.push(Span::styled(src_short, Style::default().fg(colors::PATH)));
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
        .border_style(Style::default().fg(colors::DETAIL_BORDER))
        .style(Style::default().bg(colors::DETAIL_BG));
    let details = Paragraph::new(detail_lines)
        .block(detail_block)
        .wrap(Wrap { trim: false });
    frame.render_widget(details, detail_area);

    // Footer
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            "  Enter ",
            Style::default()
                .fg(colors::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("select   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled(
            "↑↓ ",
            Style::default()
                .fg(colors::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("navigate   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(colors::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("quit", Style::default().fg(colors::DIM_GREEN)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, root[2]);
}

// ── Screen 2: Agent selection ──────────────────────────────────────────

#[allow(clippy::too_many_lines)]
fn draw_agent_screen(
    frame: &mut ratatui::Frame,
    state: &LaunchState,
    config: &AppConfig,
    cwd: &std::path::Path,
) {
    use ratatui::layout::{Alignment, Constraint, Direction, Layout};
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::{
        Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap,
    };

    let area = frame.area();
    let selected_ws = &state.workspaces[state.selected_workspace];

    let agents = state.filtered_agents();
    let list_height = (agents.len() as u16) + 2; // items + borders

    // Workspace context block height
    let ws_block_height: u16 = 3; // border top + content + border bottom

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(BANNER_HEIGHT),   // banner
            Constraint::Length(ws_block_height), // workspace context block
            Constraint::Length(1),               // gap
            Constraint::Length(list_height),     // agent list (fixed)
            Constraint::Length(1),               // gap
            Constraint::Min(6),                  // resolved access preview
            Constraint::Length(2),               // footer
        ])
        .split(area);

    // Banner
    render_banner(frame, root[0]);

    // Workspace context — centered styled block
    let ws_label = if selected_ws.name == "Current directory" {
        tui::shorten_home(&selected_ws.workspace.workdir)
    } else {
        selected_ws.name.clone()
    };
    let ws_context_area = centered_rect(root[1], 50);
    let ws_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::DETAIL_BORDER))
        .style(Style::default().bg(colors::DETAIL_BG));
    let ws_context = Paragraph::new(Line::from(vec![
        Span::styled(" workspace: ", Style::default().fg(colors::DIM_BLUE)),
        Span::styled(
            ws_label,
            Style::default()
                .fg(colors::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(ws_block)
    .alignment(Alignment::Center);
    frame.render_widget(ws_context, ws_context_area);

    // Agent list (centered, fixed height)
    let list_area = centered_rect(root[3], 50);

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

    let detail_area = centered_rect(root[5], 70);
    let detail_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors::DETAIL_BORDER))
        .style(Style::default().bg(colors::DETAIL_BG));
    let details = Paragraph::new(build_agent_detail_lines(
        config,
        cwd,
        selected_ws,
        agents.get(state.selected_agent),
    ))
    .block(detail_block)
    .wrap(Wrap { trim: false });
    frame.render_widget(details, detail_area);

    // Footer
    render_agent_footer(frame, root[6]);
}

fn build_agent_detail_lines(
    config: &AppConfig,
    cwd: &std::path::Path,
    choice: &WorkspaceChoice,
    agent: Option<&ClassSelector>,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};

    let mut detail_lines: Vec<Line<'static>> = Vec::new();

    let Some(agent) = agent else {
        detail_lines.push(Line::from(Span::styled(
            "No agents match the current filter.",
            Style::default().fg(colors::DIM_WHITE),
        )));
        return detail_lines;
    };

    detail_lines.push(Line::from(vec![
        Span::styled("agent    ", Style::default().fg(colors::BRIGHT_BLUE)),
        Span::styled(
            agent.key(),
            Style::default()
                .fg(colors::WHITE)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    match resolve_selected_workspace(config, cwd, choice, agent) {
        Ok(workspace) => {
            let workspace_destinations = choice
                .workspace
                .mounts
                .iter()
                .map(|mount| mount.dst.as_str())
                .collect::<std::collections::HashSet<_>>();

            detail_lines.push(Line::from(vec![
                Span::styled("workdir  ", Style::default().fg(colors::BRIGHT_BLUE)),
                Span::styled(
                    tui::shorten_home(&workspace.workdir),
                    Style::default().fg(colors::WHITE),
                ),
            ]));

            if !workspace.mounts.is_empty() {
                detail_lines.push(Line::from(""));
                detail_lines.push(Line::from(Span::styled(
                    "resolved mounts",
                    Style::default()
                        .fg(colors::BRIGHT_BLUE)
                        .add_modifier(Modifier::BOLD),
                )));

                for mount in &workspace.mounts {
                    let src_short = tui::shorten_home(&mount.src);
                    let dst_short = tui::shorten_home(&mount.dst);
                    let ro = if mount.readonly { " (read-only)" } else { "" };
                    let global_tag = if workspace_destinations.contains(mount.dst.as_str()) {
                        ""
                    } else {
                        " [global]"
                    };

                    let mut spans = vec![Span::styled("  ", Style::default())];
                    if src_short == dst_short {
                        spans.push(Span::styled(src_short, Style::default().fg(colors::PATH)));
                    } else {
                        spans.push(Span::styled(src_short, Style::default().fg(colors::PATH)));
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
        }
        Err(error) => {
            detail_lines.push(Line::from(""));
            detail_lines.push(Line::from(Span::styled(
                "launch preview unavailable",
                Style::default()
                    .fg(colors::BRIGHT_BLUE)
                    .add_modifier(Modifier::BOLD),
            )));
            detail_lines.push(Line::from(Span::styled(
                error.to_string(),
                Style::default().fg(colors::ERROR),
            )));
        }
    }

    detail_lines
}

fn render_agent_footer(frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
    use ratatui::layout::Alignment;
    use ratatui::style::{Modifier, Style};
    use ratatui::text::{Line, Span};
    use ratatui::widgets::Paragraph;

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(
            "  Enter ",
            Style::default()
                .fg(colors::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("load   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled(
            "↑↓ ",
            Style::default()
                .fg(colors::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("navigate   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled(
            "Type ",
            Style::default()
                .fg(colors::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("to filter   ", Style::default().fg(colors::DIM_GREEN)),
        Span::styled(
            "Esc ",
            Style::default()
                .fg(colors::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("back", Style::default().fg(colors::DIM_GREEN)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(footer, area);
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
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
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
                last_agent: None,
            },
        );

        let state = LaunchState::new(&config, &project_dir).unwrap();
        assert_eq!(state.selected_workspace_name(), Some("big-monorepo"));
    }

    #[test]
    fn preselects_saved_workspace_for_nested_directory_under_mount_root() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let nested_dir = project_dir.join("src/lib");
        std::fs::create_dir_all(&nested_dir).unwrap();
        let nested_dir = nested_dir.canonicalize().unwrap();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: "/workspace".to_string(),
                mounts: vec![crate::workspace::MountConfig {
                    src: project_dir.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
            },
        );

        let state = LaunchState::new(&config, &nested_dir).unwrap();
        assert_eq!(state.selected_workspace_name(), Some("big-monorepo"));
    }

    #[test]
    fn preselects_saved_workspace_from_host_workdir_root() {
        let temp = tempfile::tempdir().unwrap();
        let workspace_root = temp.path().join("monorepo");
        let repo_dir = workspace_root.join("jackin");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let workspace_root = workspace_root.canonicalize().unwrap();

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            "agent-smith".to_string(),
            crate::config::AgentSource {
                git: "https://github.com/jackin-project/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.workspaces.insert(
            "big-monorepo".to_string(),
            crate::workspace::WorkspaceConfig {
                workdir: workspace_root.display().to_string(),
                mounts: vec![crate::workspace::MountConfig {
                    src: repo_dir.canonicalize().unwrap().display().to_string(),
                    dst: "/workspace/jackin".to_string(),
                    readonly: false,
                }],
                allowed_agents: vec!["agent-smith".to_string()],
                default_agent: Some("agent-smith".to_string()),
                last_agent: None,
            },
        );

        let state = LaunchState::new(&config, &workspace_root).unwrap();
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
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        let filtered = state.filtered_agents();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].key(), "chainargos/the-architect");
    }

    // ── Phase 0 gap-fill: agent-filter composition ─────────────────────────
    //
    // These tests pin the composition the TUI relies on:
    //
    //   configured_agents  →  eligible_agents_for_saved_workspace
    //                     (allowed_agents filter)  →
    //                     workspace.allowed_agents  →
    //                     filtered_agents          (agent_query filter)  →
    //                     on-screen result
    //
    // Invariants the plan's Phase 0 calls out for the Phase 6 unification:
    //
    //   1. An empty `allowed_agents` list means "any configured agent."
    //   2. A non-empty `allowed_agents` list strictly narrows to the named
    //      set, and never resurrects an unconfigured ("ghost") name.
    //   3. The query filter composes with — never widens — the post-eligibility
    //      set. A key not in `workspace.allowed_agents` cannot be recovered
    //      by any query string.
    //   4. An empty query returns the full post-eligibility set.
    //   5. A query that matches a subset of the eligible set returns exactly
    //      that subset (does not drop matches, does not add non-matches).

    fn agent_source_stub() -> crate::config::AgentSource {
        crate::config::AgentSource {
            git: "https://example.invalid/org/repo.git".to_string(),
            trusted: true,
            claude: None,
        }
    }

    fn workspace_with_allowed(allowed: &[&str]) -> crate::workspace::WorkspaceConfig {
        crate::workspace::WorkspaceConfig {
            workdir: "/work".to_string(),
            mounts: vec![],
            allowed_agents: allowed.iter().map(|s| (*s).to_string()).collect(),
            default_agent: None,
            last_agent: None,
        }
    }

    #[test]
    fn eligible_agents_returns_all_configured_when_allowed_list_empty() {
        let mut config = crate::config::AppConfig::default();
        config
            .agents
            .insert("alice".to_string(), agent_source_stub());
        config.agents.insert("bob".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&[]);
        let eligible = eligible_agents_for_saved_workspace(&config, &ws);
        let keys: Vec<String> = eligible
            .iter()
            .map(crate::selector::ClassSelector::key)
            .collect();

        assert_eq!(eligible.len(), 2, "empty allowed_agents must mean 'any'");
        assert!(keys.contains(&"alice".to_string()));
        assert!(keys.contains(&"bob".to_string()));
    }

    #[test]
    fn eligible_agents_narrows_to_allowed_list_when_non_empty() {
        let mut config = crate::config::AppConfig::default();
        config
            .agents
            .insert("alice".to_string(), agent_source_stub());
        config.agents.insert("bob".to_string(), agent_source_stub());
        config
            .agents
            .insert("carol".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&["alice", "carol"]);
        let eligible = eligible_agents_for_saved_workspace(&config, &ws);
        let keys: Vec<String> = eligible
            .iter()
            .map(crate::selector::ClassSelector::key)
            .collect();

        assert_eq!(eligible.len(), 2);
        assert!(keys.contains(&"alice".to_string()));
        assert!(keys.contains(&"carol".to_string()));
        assert!(!keys.contains(&"bob".to_string()));
    }

    #[test]
    fn eligible_agents_drops_ghost_name_not_in_config() {
        // `allowed_agents` references an agent that was removed from config.
        // The eligibility set must not fabricate a selector for it.
        let mut config = crate::config::AppConfig::default();
        config
            .agents
            .insert("alice".to_string(), agent_source_stub());

        let ws = workspace_with_allowed(&["ghost"]);
        let eligible = eligible_agents_for_saved_workspace(&config, &ws);

        assert!(
            eligible.is_empty(),
            "eligibility must not resurrect a name absent from config.agents"
        );
    }

    #[test]
    fn empty_query_returns_full_post_eligibility_set() {
        let state = LaunchState {
            stage: LaunchStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: String::new(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![
                    crate::selector::ClassSelector::new(None, "alice"),
                    crate::selector::ClassSelector::new(None, "bob"),
                ],
                default_agent: None,
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        let filtered = state.filtered_agents();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn query_cannot_reintroduce_agent_excluded_by_allowed_list() {
        // `state.workspaces[_].allowed_agents` already reflects the
        // eligibility filter. An agent absent here cannot be resurrected
        // by *any* query string — the query only narrows, never widens.
        let state = LaunchState {
            stage: LaunchStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: "bob".to_string(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![crate::selector::ClassSelector::new(None, "alice")],
                default_agent: None,
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        assert!(
            state.filtered_agents().is_empty(),
            "query must not resurrect an excluded agent"
        );
    }

    #[test]
    fn query_narrows_within_allowed_set_without_dropping_matches() {
        // Multiple eligible agents; query matches a subset. Every matching
        // agent must still appear; no non-matching agent may sneak through.
        let state = LaunchState {
            stage: LaunchStage::Agent,
            selected_workspace: 0,
            selected_agent: 0,
            agent_query: "smith".to_string(),
            workspaces: vec![WorkspaceChoice {
                name: "Current directory".to_string(),
                workspace: crate::workspace::ResolvedWorkspace {
                    label: "/tmp/project".to_string(),
                    workdir: "/tmp/project".to_string(),
                    mounts: vec![],
                },
                allowed_agents: vec![
                    crate::selector::ClassSelector::new(None, "agent-smith"),
                    crate::selector::ClassSelector::new(None, "agent-brown"),
                    crate::selector::ClassSelector::new(None, "smithy"),
                ],
                default_agent: None,
                last_agent: None,
                global_mounts: vec![],
                input: LoadWorkspaceInput::CurrentDir,
            }],
        };

        let filtered = state.filtered_agents();
        let keys: Vec<String> = filtered
            .iter()
            .map(crate::selector::ClassSelector::key)
            .collect();

        assert_eq!(
            filtered.len(),
            2,
            "query 'smith' should match exactly 2 of 3 allowed agents"
        );
        assert!(keys.contains(&"agent-smith".to_string()));
        assert!(keys.contains(&"smithy".to_string()));
        assert!(!keys.contains(&"agent-brown".to_string()));
    }

    #[test]
    fn footer_text_matches_stage_behavior() {
        assert!(footer_text(LaunchStage::Workspace).contains("Enter"));
        assert!(footer_text(LaunchStage::Workspace).contains("quit"));
        assert!(footer_text(LaunchStage::Agent).contains("Enter"));
        assert!(footer_text(LaunchStage::Agent).contains("back"));
        assert!(footer_text(LaunchStage::Agent).contains("filter"));
    }

    #[test]
    fn agent_preview_includes_selector_scoped_global_mounts() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path().join("project");
        let shared_dir = temp.path().join("shared-cache");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::create_dir_all(&shared_dir).unwrap();

        let selector = crate::selector::ClassSelector::new(Some("chainargos"), "agent-smith");

        let mut config = crate::config::AppConfig::default();
        config.agents.insert(
            selector.key(),
            crate::config::AgentSource {
                git: "https://github.com/chainargos/jackin-agent-smith.git".to_string(),
                trusted: true,
                claude: None,
            },
        );
        config.add_mount(
            "shared-cache",
            crate::workspace::MountConfig {
                src: shared_dir.canonicalize().unwrap().display().to_string(),
                dst: "/cache".to_string(),
                readonly: true,
            },
            Some("chainargos/*"),
        );

        let project_dir = project_dir.canonicalize().unwrap();
        let choice = WorkspaceChoice {
            name: "Current directory".to_string(),
            workspace: crate::workspace::ResolvedWorkspace {
                label: project_dir.display().to_string(),
                workdir: project_dir.display().to_string(),
                mounts: vec![crate::workspace::MountConfig {
                    src: project_dir.display().to_string(),
                    dst: project_dir.display().to_string(),
                    readonly: false,
                }],
            },
            allowed_agents: vec![selector.clone()],
            default_agent: None,
            last_agent: None,
            global_mounts: vec![],
            input: LoadWorkspaceInput::CurrentDir,
        };

        let details = build_agent_detail_lines(&config, &project_dir, &choice, Some(&selector));
        let rendered = details
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("resolved mounts"));
        assert!(rendered.contains("/cache"));
        assert!(rendered.contains("[global]"));
    }
}
