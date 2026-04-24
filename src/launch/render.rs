use super::preview::build_agent_detail_lines;
use super::state::LaunchState;
use crate::config::AppConfig;
use crate::tui;

#[cfg(test)]
use super::state::LaunchStage;

// ── Color palette (matching CLI banner) ────────────────────────────────

pub(super) mod colors {
    use ratatui::style::Color;

    pub const BRIGHT_BLUE: Color = Color::Rgb(100, 149, 237); // circuit lines, labels
    pub const DIM_BLUE: Color = Color::Rgb(75, 105, 145); // borders, subtitle
    pub const DETAIL_BORDER: Color = Color::Rgb(60, 75, 90); // details panel border
    pub const DETAIL_BG: Color = Color::Rgb(15, 17, 25); // details panel background
    pub const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65); // highlight
    pub const DIM_GREEN: Color = Color::Rgb(0, 140, 30); // footer hints
    pub const WHITE: Color = Color::Rgb(255, 255, 255);
    pub const DIM_WHITE: Color = Color::Rgb(180, 180, 180);
    pub const PATH: Color = Color::Rgb(220, 190, 120); // paths (warm amber)
    pub const PATH_DST: Color = Color::Rgb(150, 180, 220); // mount destination
    pub const DARK_BG: Color = Color::Rgb(20, 20, 30); // subtle bg for selected
    pub const ERROR: Color = Color::Rgb(230, 120, 120);
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

// ── Screen: Agent selection ────────────────────────────────────────────

#[allow(clippy::too_many_lines)]
pub(super) fn draw_agent_screen(
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
fn footer_text(stage: &LaunchStage) -> &'static str {
    match stage {
        LaunchStage::Agent => "Enter load   ↑↓ navigate   Type to filter   Esc back",
        LaunchStage::Manager(_) => "↑↓ · Enter launch · e edit · n new · d delete · q quit",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_text_matches_stage_behavior() {
        assert!(footer_text(&LaunchStage::Agent).contains("Enter"));
        assert!(footer_text(&LaunchStage::Agent).contains("back"));
        assert!(footer_text(&LaunchStage::Agent).contains("filter"));
    }
}
