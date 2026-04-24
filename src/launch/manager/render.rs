//! Render functions for the workspace manager TUI.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use super::super::widgets::{confirm, file_browser, text_input, workdir_pick};
use super::state::{
    EditorMode, EditorState, EditorTab, ManagerStage, ManagerState, Modal, WorkspaceSummary,
};

const PHOSPHOR_GREEN: Color = Color::Rgb(0, 255, 65);
const PHOSPHOR_DIM: Color = Color::Rgb(0, 140, 30);
const PHOSPHOR_DARK: Color = Color::Rgb(0, 80, 18);
const WHITE: Color = Color::Rgb(255, 255, 255);

pub fn render(frame: &mut Frame, state: &ManagerState<'_>) {
    // Some stages render their own full-screen layout.
    if let ManagerStage::Editor(editor) = &state.stage {
        render_editor(frame, editor);
        return;
    }

    // List / CreatePrelude / ConfirmDelete share the list-like chrome.
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(10),   // body
            Constraint::Length(2), // footer
        ])
        .split(area);

    render_header(frame, chunks[0], "manage workspaces");

    if matches!(&state.stage, ManagerStage::List) {
        render_list_body(frame, chunks[1], state);
    }

    render_footer_hint(
        frame,
        chunks[2],
        "↑↓ · Enter launch · e edit · n new · d delete · q quit",
    );
}

fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    let line = Line::from(vec![
        Span::styled("▓▓▓▓ ", Style::default().fg(PHOSPHOR_GREEN)),
        Span::styled(
            "JACKIN",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ),
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
    let mut items: Vec<ListItem> = state
        .workspaces
        .iter()
        .map(|w| ListItem::new(Line::from(w.name.as_str())))
        .collect();
    items.push(ListItem::new(Line::from(Span::styled(
        "+ New workspace",
        Style::default().fg(WHITE),
    ))));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PHOSPHOR_DARK)),
        )
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
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PHOSPHOR_DARK));
        frame.render_widget(block, columns[1]);
    }

    // Toast overlay — rendered last so it appears on top.
    if let Some(toast) = &state.toast {
        render_toast(frame, area, toast);
    }
}

fn render_toast(frame: &mut Frame, area: Rect, toast: &super::state::Toast) {
    use super::state::ToastKind;
    let elapsed = toast.shown_at.elapsed();
    // Auto-expire after 3 seconds — caller should clear before that,
    // but defensively skip rendering if we're past.
    if elapsed > std::time::Duration::from_secs(3) {
        return;
    }

    let (prefix, color) = match toast.kind {
        ToastKind::Success => ("✓ ", PHOSPHOR_GREEN),
        ToastKind::Error => ("✗ ", Color::Rgb(255, 94, 122)),
    };
    let mut style = Style::default().fg(color).add_modifier(Modifier::BOLD);
    // Shimmer: first 400ms is bright-white flicker, then settles.
    if elapsed < std::time::Duration::from_millis(400) {
        style = style.fg(WHITE);
    }
    let line = Line::from(Span::styled(format!("{}{}", prefix, toast.message), style));
    let banner_area = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: 1,
    };
    frame.render_widget(ratatui::widgets::Clear, banner_area);
    frame.render_widget(Paragraph::new(line), banner_area);
}

fn render_details_pane(frame: &mut Frame, area: Rect, ws: &WorkspaceSummary) {
    let lines = vec![
        Line::from(vec![
            Span::styled("workdir ", Style::default().fg(WHITE)),
            Span::raw(ws.workdir.clone()),
        ]),
        Line::from(vec![
            Span::styled("mounts  ", Style::default().fg(WHITE)),
            Span::raw(format!(
                "{} ({} readonly)",
                ws.mount_count, ws.readonly_mount_count
            )),
        ]),
        Line::from(vec![
            Span::styled("agents  ", Style::default().fg(WHITE)),
            Span::raw(format!("{} allowed", ws.allowed_agent_count)),
        ]),
        Line::from(vec![
            Span::styled("last    ", Style::default().fg(WHITE)),
            Span::raw(
                ws.last_agent
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
            ),
        ]),
    ];
    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PHOSPHOR_DARK))
                .title(format!(" Details — {} ", ws.name)),
        )
        .style(Style::default().fg(PHOSPHOR_GREEN));
    frame.render_widget(p, area);
}

fn render_footer_hint(frame: &mut Frame, area: Rect, hint: &str) {
    let p = Paragraph::new(Span::styled(
        hint.to_string(),
        Style::default().fg(PHOSPHOR_DIM),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(p, area);
}

// ── Editor stage ────────────────────────────────────────────────────

pub fn render_editor(frame: &mut Frame, state: &EditorState<'_>) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Length(2), // tab strip
            Constraint::Min(8),    // tab body
            Constraint::Length(2), // footer
        ])
        .split(area);

    let title = match &state.mode {
        EditorMode::Edit { name } => format!("edit · {name}"),
        EditorMode::Create => "new workspace".to_string(),
    };
    render_header(frame, chunks[0], &title);

    render_tab_strip(frame, chunks[1], state.active_tab);

    match state.active_tab {
        EditorTab::General => render_general_tab(frame, chunks[2], state),
        EditorTab::Mounts => render_mounts_tab(frame, chunks[2], state),
        EditorTab::Agents => render_agents_tab(frame, chunks[2], state),
        EditorTab::Secrets => render_secrets_stub(frame, chunks[2]),
    }

    let footer = if state.is_dirty() {
        format!(
            "Tab next · ↑↓ field · Enter edit · s save ({} changes) · Esc discard",
            state.change_count()
        )
    } else {
        "Tab next · ↑↓ field · Enter edit · s save · Esc back".to_string()
    };
    render_footer_hint(frame, chunks[3], &footer);

    // Error banner overlay — top line of the body.
    if let Some(err) = &state.error_banner {
        let banner_area = Rect {
            x: chunks[2].x,
            y: chunks[2].y,
            width: chunks[2].width,
            height: 1,
        };
        let banner = Paragraph::new(format!("✗ {err}")).style(
            Style::default()
                .fg(Color::Rgb(255, 94, 122))
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(ratatui::widgets::Clear, banner_area);
        frame.render_widget(banner, banner_area);
    }
}

fn render_tab_strip(frame: &mut Frame, area: Rect, active: EditorTab) {
    let labels = [
        (EditorTab::General, "General"),
        (EditorTab::Mounts, "Mounts"),
        (EditorTab::Agents, "Agents"),
        (EditorTab::Secrets, "Secrets ⏳"),
    ];
    let mut spans = Vec::new();
    for (tab, label) in labels {
        let style = if tab == active {
            Style::default()
                .bg(PHOSPHOR_GREEN)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else if tab == EditorTab::Secrets {
            Style::default()
                .fg(Color::Rgb(90, 90, 90))
                .add_modifier(Modifier::ITALIC)
        } else {
            Style::default().fg(PHOSPHOR_DIM)
        };
        spans.push(Span::styled(format!(" {label} "), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_general_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));

    let name_value = match &state.mode {
        EditorMode::Edit { name } => name.as_str(),
        EditorMode::Create => state.pending_name.as_deref().unwrap_or("(new)"),
    };

    let rows = vec![
        render_field_row("name", name_value, false),
        render_field_row(
            "workdir",
            &state.pending.workdir,
            state.pending.workdir != state.original.workdir,
        ),
        render_field_row(
            "default agent",
            state.pending.default_agent.as_deref().unwrap_or("(none)"),
            state.pending.default_agent != state.original.default_agent,
        ),
        Line::from(vec![
            Span::styled("  last used      ", Style::default().fg(WHITE)),
            Span::styled(
                state
                    .original
                    .last_agent
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            Span::styled(
                " (read-only)",
                Style::default()
                    .fg(PHOSPHOR_DIM)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(rows).block(block), area);
}

fn render_field_row(label: &str, value: &str, dirty: bool) -> Line<'static> {
    let mut spans = vec![
        Span::styled(format!("  {label:15}"), Style::default().fg(WHITE)),
        Span::raw(value.to_string()),
    ];
    if dirty {
        spans.push(Span::styled(
            "    ● unsaved",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

fn render_mounts_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let mut lines: Vec<Line> = state
        .pending
        .mounts
        .iter()
        .map(|m| {
            let ro = if m.readonly { " (ro)" } else { " (rw)" };
            Line::from(format!("  {} → {}{}", m.src, m.dst, ro))
        })
        .collect();
    lines.push(Line::from(Span::styled(
        "  + Add mount    − Remove selected",
        Style::default()
            .fg(PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC),
    )));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_agents_tab(frame: &mut Frame, area: Rect, state: &EditorState<'_>) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let header = Line::from(Span::styled(
        "  allowed? · default ·  agent",
        Style::default().fg(WHITE),
    ));
    let mut lines = vec![header];
    for agent in &state.pending.allowed_agents {
        let is_default = state.pending.default_agent.as_deref() == Some(agent);
        let star = if is_default { "★" } else { " " };
        lines.push(Line::from(format!("  [x]          {star}        {agent}")));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_secrets_stub(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK));
    let body = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Secrets management lands in PR 3 of this series.",
            Style::default()
                .fg(PHOSPHOR_DIM)
                .add_modifier(Modifier::ITALIC),
        )),
    ];
    frame.render_widget(Paragraph::new(body).block(block), area);
}

// ── Modal dispatcher ────────────────────────────────────────────────

pub fn render_modal(frame: &mut Frame, modal: &Modal<'_>) {
    let area = frame.area();
    let modal_area = centered_rect(area, 60, 30);

    match modal {
        Modal::TextInput { state, .. } => text_input::render(frame, modal_area, state),
        Modal::FileBrowser { state, .. } => file_browser::render(frame, modal_area, state),
        Modal::WorkdirPick { state } => workdir_pick::render(frame, modal_area, state),
        Modal::Confirm { state, .. } => confirm::render(frame, modal_area, state),
    }
}

const fn centered_rect(outer: Rect, pct_w: u16, pct_h: u16) -> Rect {
    let w = outer.width * pct_w / 100;
    let h = outer.height * pct_h / 100;
    Rect {
        x: outer.x + (outer.width.saturating_sub(w)) / 2,
        y: outer.y + (outer.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}
