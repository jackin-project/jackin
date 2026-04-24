//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::state::{ManagerStage, ManagerState, WorkspaceSummary};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

pub fn render(frame: &mut Frame, state: &ManagerState<'_>) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // header
            Constraint::Min(10),    // body
            Constraint::Length(2),  // footer
        ])
        .split(area);

    render_header(frame, chunks[0], "manage workspaces");

    match &state.stage {
        ManagerStage::List => render_list_body(frame, chunks[1], state),
        _ => {} // other stages rendered by other functions (Tasks 12–13)
    }

    render_footer_hint(frame, chunks[2], "↑↓ · Enter edit · n new · d delete · Esc back to launcher");
}

fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    let line = Line::from(vec![
        Span::styled("▓▓▓▓ ", Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled("JACKIN", Style::default().fg(WHITE).add_modifier(Modifier::BOLD)),
        Span::raw("     · "),
        Span::styled(title.to_string(), Style::default().fg(PHOSPHOR_DIM)),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Left), area);
}

fn render_list_body(frame: &mut Frame, area: Rect, state: &ManagerState<'_>) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Left: list of workspaces + [+ New workspace] sentinel.
    let mut items: Vec<ListItem> = state.workspaces.iter().map(|w| {
        ListItem::new(Line::from(w.name.as_str()))
    }).collect();
    items.push(ListItem::new(Line::from(Span::styled(
        "+ New workspace",
        Style::default().fg(WHITE),
    ))));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK)))
        .style(Style::default().fg(PHOSPHOR_GREEN))
        .highlight_style(Style::default().bg(PHOSPHOR_GREEN).fg(Color::Black))
        .highlight_symbol("▸ ");

    let mut ls = ListState::default();
    ls.select(Some(state.selected));
    frame.render_stateful_widget(list, columns[0], &mut ls);

    // Right: details pane for currently-selected workspace.
    if let Some(ws) = state.workspaces.get(state.selected) {
        render_details_pane(frame, columns[1], ws);
    } else {
        // [+ New workspace] selected — right pane is empty.
        let block = Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK));
        frame.render_widget(block, columns[1]);
    }
}

fn render_details_pane(frame: &mut Frame, area: Rect, ws: &WorkspaceSummary) {
    let lines = vec![
        Line::from(vec![Span::styled("workdir ", Style::default().fg(WHITE)), Span::raw(ws.workdir.clone())]),
        Line::from(vec![
            Span::styled("mounts  ", Style::default().fg(WHITE)),
            Span::raw(format!("{} ({} readonly)", ws.mount_count, ws.readonly_mount_count)),
        ]),
        Line::from(vec![
            Span::styled("agents  ", Style::default().fg(WHITE)),
            Span::raw(format!("{} allowed", ws.allowed_agent_count)),
        ]),
        Line::from(vec![
            Span::styled("last    ", Style::default().fg(WHITE)),
            Span::raw(ws.last_agent.clone().unwrap_or_else(|| "(none)".to_string())),
        ]),
    ];
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(PHOSPHOR_DARK)).title(format!(" Details — {} ", ws.name)))
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

fn render_footer_hint(frame: &mut Frame, area: Rect, hint: &str) {
    let p = Paragraph::new(Span::styled(hint.to_string(), Style::default().fg(PHOSPHOR_DIM)))
        .alignment(Alignment::Center);
    frame.render_widget(p, area);
}
