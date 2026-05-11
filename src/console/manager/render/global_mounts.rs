use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::{
    FooterItem, PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, WHITE, render_footer, render_header,
};
use crate::console::manager::render::list::mount_display_paths;
use crate::console::manager::state::{GlobalMountModal, GlobalMountsState};

#[allow(clippy::too_many_lines)]
pub(super) fn render_global_mounts(frame: &mut Frame, state: &GlobalMountsState<'_>) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);
    render_header(frame, chunks[0], "global config · mounts");

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PHOSPHOR_DARK))
        .title(Span::styled(
            " Global mounts ",
            Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
        ));
    let mut lines = vec![Line::from(Span::styled(
        "  Name                 Destination                    Mode Scope",
        Style::default().fg(WHITE),
    ))];
    if state.pending.is_empty() {
        lines.push(Line::from(Span::styled(
            "  (none)",
            Style::default().fg(PHOSPHOR_DIM),
        )));
    }
    for (i, row) in state.pending.iter().enumerate() {
        let selected = i == state.selected;
        let prefix = if selected { "▸ " } else { "  " };
        let style = if selected {
            Style::default()
                .fg(PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(PHOSPHOR_GREEN)
        };
        let mode = if row.mount.readonly { "ro" } else { "rw" };
        let scope = row.scope.as_deref().unwrap_or("global");
        let (destination, host_source) = mount_display_paths(&row.mount);
        lines.push(Line::from(Span::styled(
            format!(
                "{prefix}{:<20} {:<30} {:<4} {:<16}",
                truncate(&row.name, 20),
                truncate(&destination, 30),
                mode,
                truncate(scope, 16)
            ),
            style,
        )));
        if let Some(host_source) = host_source {
            lines.push(Line::from(Span::styled(
                format!("  {:<20} {}", "", truncate(&host_source, 64)),
                Style::default().fg(PHOSPHOR_DIM),
            )));
        }
    }
    if let Some(err) = &state.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {err}"),
            Style::default().fg(ratatui::style::Color::Rgb(255, 94, 122)),
        )));
    }
    let content_width = super::max_line_width(&lines);
    let scroll_x = super::effective_scroll_x(
        content_width,
        chunks[1].width.saturating_sub(2) as usize,
        state.scroll_x,
    );
    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((0, scroll_x)),
        chunks[1],
    );
    super::render_horizontal_scrollbar(frame, chunks[1], content_width, scroll_x);

    let mut items = vec![
        FooterItem::Key("A"),
        FooterItem::Text("add"),
        FooterItem::Sep,
        FooterItem::Key("N"),
        FooterItem::Text("rename"),
        FooterItem::Sep,
        FooterItem::Key("1"),
        FooterItem::Text("source"),
        FooterItem::Sep,
        FooterItem::Key("2"),
        FooterItem::Text("destination"),
        FooterItem::Sep,
        FooterItem::Key("3"),
        FooterItem::Text("scope"),
        FooterItem::Sep,
        FooterItem::Key("R"),
        FooterItem::Text("ro/rw"),
        FooterItem::Sep,
        FooterItem::Key("D"),
        FooterItem::Text("remove"),
        FooterItem::GroupSep,
        FooterItem::Key("S"),
        FooterItem::Text("save global config"),
        FooterItem::GroupSep,
        FooterItem::Key("←/→"),
        FooterItem::Text("scroll"),
    ];
    if state.is_dirty() {
        items.push(FooterItem::Dyn("(unsaved)".to_string()));
    }
    items.extend([
        FooterItem::GroupSep,
        FooterItem::Key("Esc"),
        FooterItem::Text("back"),
    ]);
    render_footer(frame, chunks[2], &items);
}

pub(super) fn render_global_mount_modal(frame: &mut Frame, modal: &mut GlobalMountModal<'_>) {
    let area = super::centered_rect_fixed(frame.area(), 70, 7);
    match modal {
        GlobalMountModal::Text { state, .. } => {
            crate::console::widgets::text_input::render(frame, area, state);
        }
        GlobalMountModal::ConfirmRemove { state }
        | GlobalMountModal::ConfirmSave { state }
        | GlobalMountModal::ConfirmSensitive { state } => {
            crate::console::widgets::confirm::render(frame, area, state);
        }
    }
}

fn truncate(value: &str, width: usize) -> String {
    let mut out: String = value.chars().take(width).collect();
    if value.chars().count() > width && width > 1 {
        out.pop();
        out.push('…');
    }
    out
}
