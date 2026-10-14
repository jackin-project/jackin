// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::git_prompt::render_git_prompt;
use super::state::FileBrowserState;
use termrock::widgets::{List, ListRow, ListState, Panel, PanelChrome};

fn render_constraints(has_rejection: bool) -> Vec<ratatui::layout::Constraint> {
    use ratatui::layout::Constraint;
    if has_rejection {
        vec![Constraint::Length(1), Constraint::Min(3)]
    } else {
        vec![Constraint::Min(3)]
    }
}

pub fn listing_rect(modal_area: Rect, has_rejection: bool) -> Rect {
    use ratatui::layout::{Direction, Layout};
    let constraints = render_constraints(has_rejection);
    let listing_idx = usize::from(has_rejection);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(modal_area);
    chunks[listing_idx]
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &FileBrowserState) {
    use ratatui::layout::{Alignment, Direction, Layout};

    frame.render_widget(ratatui::widgets::Clear, area);

    let rejection = state.rejected_reason.as_ref();
    let constraints = render_constraints(rejection.is_some());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let listing_idx = rejection.map_or(0, |reason| {
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!("\u{2717} {reason}"),
                termrock::style::DesignSystem::default().style(termrock::style::Role::Danger),
            ))
            .alignment(Alignment::Center),
            chunks[0],
        );
        1
    });

    render_listing(frame, chunks[listing_idx], state);

    if state.pending_git_prompt.is_some() {
        render_git_prompt(frame, chunks[listing_idx], state);
    }
}

fn render_listing(frame: &mut Frame<'_>, area: Rect, state: &FileBrowserState) {
    let title = format!(
        " {} ",
        jackin_core::shorten_home(&state.cwd.display().to_string())
    );
    let theme = termrock::style::DesignSystem::default();
    let panel = Panel::new(&theme)
        .title(&title)
        .emphasis(if state.pending_git_prompt.is_some() {
            PanelChrome::Normal
        } else {
            PanelChrome::Focused
        });
    let inner = Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };
    frame.render_widget(&panel, area);

    let selected = state
        .pending_git_prompt
        .is_none()
        .then_some(state.list_state.selected().copied())
        .flatten();
    let base_style = Style::default().fg(termrock::style::DesignSystem::default()
        .style(termrock::style::Role::Text)
        .fg
        .unwrap_or_default());
    let git_suffix_style = Style::default()
        .fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default())
        .add_modifier(Modifier::BOLD);

    let rows: Vec<ListRow<'_, usize>> = state
        .entries
        .iter()
        .enumerate()
        .map(|(id, e)| {
            let name_slash = if e.is_parent {
                "../".to_owned()
            } else {
                format!("{}/", e.name)
            };
            let line = if e.is_git {
                Line::from(vec![
                    Span::styled(name_slash, base_style),
                    Span::styled(" (git)", git_suffix_style),
                ])
            } else {
                Line::from(Span::styled(name_slash, base_style))
            };
            ListRow::item(id, line)
        })
        .collect();
    let mut list_state = ListState::new(selected);
    let scrollbar_gutter = u16::from(state.entries.len() > usize::from(inner.height));
    let list_area = Rect {
        width: inner
            .width
            .saturating_add(scrollbar_gutter)
            .min(area.right().saturating_sub(inner.x)),
        ..inner
    };
    frame.render_stateful_widget(&List::new(&rows, &theme), list_area, &mut list_state);
}

#[cfg(test)]
mod tests;
