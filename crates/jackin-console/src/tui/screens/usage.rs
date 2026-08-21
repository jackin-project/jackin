// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Simple Console Usage route.
//!
//! Rust supplies already ordered account/window values. This module owns only
//! the Console split, focus, and Capsule-shaped meter adaptation.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::tui::state::ManagerState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageWindow {
    pub label: String,
    pub value: String,
    pub reset: String,
    pub remaining_percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageAccount {
    pub provider: String,
    pub account: String,
    pub status: String,
    pub windows: Vec<UsageWindow>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageScreenState {
    pub accounts: Vec<UsageAccount>,
    pub selected: usize,
    pub detail: bool,
    pub scroll: u16,
    pub notice: Option<String>,
}

impl UsageScreenState {
    /// Project the Rust-owned canonical publication into Console rows.
    ///
    /// The Console owns only layout. Provider/account identity, lifecycle,
    /// ordering, and quota labels remain in the protocol projection.
    pub fn from_projection(projection: &jackin_protocol::usage_broker::UsageProjectionV1) -> Self {
        use jackin_protocol::usage_broker::{UsageLifecycleV1, UsagePercent};

        let mut accounts = Vec::new();
        for provider in &projection.providers {
            for account in &provider.accounts {
                let mut status = account
                    .status_label
                    .clone()
                    .unwrap_or_else(|| lifecycle_label(account.lifecycle).to_owned());
                if account.freshness.is_stale && account.lifecycle == UsageLifecycleV1::Available {
                    status = "stale".to_owned();
                }
                let windows = account
                    .windows
                    .iter()
                    .map(|window| UsageWindow {
                        label: window.label.clone(),
                        value: window.value_label.clone(),
                        reset: window.reset_label.clone(),
                        remaining_percent: window.remaining_percent.map(UsagePercent::get),
                    })
                    .collect();
                accounts.push(UsageAccount {
                    provider: provider.display_name.clone(),
                    account: account.display_label.clone(),
                    status,
                    windows,
                });
            }
        }

        let notice = if projection.unresolved.is_empty() {
            None
        } else {
            Some(format!(
                "{} configured capability(s) unresolved",
                projection.unresolved.len()
            ))
        };
        Self {
            accounts,
            notice,
            ..Self::default()
        }
    }

    pub fn set_accounts(&mut self, accounts: Vec<UsageAccount>) {
        self.accounts = accounts;
        self.selected = self.selected.min(self.accounts.len());
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.accounts.is_empty() {
            return;
        }
        let len = self.accounts.len().saturating_add(1);
        let current = self.selected.min(len - 1);
        self.selected = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current
                .saturating_add(delta.cast_unsigned())
                .min(len.saturating_sub(1))
        };
    }

    pub fn selected_account(&self) -> Option<&UsageAccount> {
        (self.selected > 0).then(|| self.accounts.get(self.selected - 1))?
    }

    fn overview_selected(&self) -> bool {
        self.selected == 0
    }
}

fn lifecycle_label(lifecycle: jackin_protocol::usage_broker::UsageLifecycleV1) -> &'static str {
    use jackin_protocol::usage_broker::UsageLifecycleV1;
    match lifecycle {
        UsageLifecycleV1::Available => "available",
        UsageLifecycleV1::AgentUninitialized => "not started",
        UsageLifecycleV1::NeedsLogin => "needs login",
        UsageLifecycleV1::NeedsSecret => "needs secret",
        UsageLifecycleV1::Unsupported => "unsupported",
        UsageLifecycleV1::Unavailable => "unavailable",
        UsageLifecycleV1::Error => "error",
    }
}

pub fn handle_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let Some(screen) = state.usage_screen.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => state.usage_screen = None,
        KeyCode::Up | KeyCode::Char('k') => screen.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => screen.move_selection(1),
        KeyCode::Enter => screen.detail = !screen.detail,
        KeyCode::Char('r') => {}
        KeyCode::PageUp => screen.scroll = screen.scroll.saturating_sub(5),
        KeyCode::PageDown => {
            screen.scroll = screen.scroll.saturating_add(5);
        }
        _ => {}
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let body = crate::tui::view::workspace_frame_areas(area).body;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    render_account_list(frame, columns[0], state);
    render_detail(frame, columns[1], state);
}

fn render_account_list(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let Some(screen) = state.usage_screen.as_ref() else {
        return;
    };
    let focused = true;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::DarkGray)
        });
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if screen.accounts.is_empty() {
        frame.render_widget(
            Paragraph::new("No providers configured.\n\nPress R to refresh.")
                .style(Style::default().fg(Color::DarkGray))
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let mut lines = Vec::new();
    let meter_width = inner.width.saturating_sub(8) as usize;
    lines.push(Line::from(Span::styled(
        format!(
            "{}Overview",
            if screen.overview_selected() {
                "▸  "
            } else {
                "   "
            }
        ),
        row_style(screen.overview_selected()),
    )));
    lines.push(Line::from(""));
    for (index, account) in screen.accounts.iter().enumerate() {
        if index == 0 || screen.accounts[index - 1].provider != account.provider {
            lines.push(Line::from(Span::styled(
                format!("  {}", account.provider),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )));
        }
        let selected = index.saturating_add(1) == screen.selected;
        let cursor = if selected { "▸ " } else { "  " };
        let summary = account
            .windows
            .first()
            .and_then(|window| window.remaining_percent)
            .map_or_else(
                || account.status.clone(),
                |percent| format!("{percent}% left"),
            );
        lines.push(Line::from(Span::styled(
            format!("  {cursor}{}", account.account),
            row_style(selected),
        )));
        lines.push(Line::from(Span::styled(
            format!("      {} · {}", account.status, summary),
            Style::default().fg(Color::DarkGray),
        )));
        if let Some(window) = account.windows.first() {
            lines.push(Line::from(Span::styled(
                meter_line(meter_width, window.remaining_percent),
                meter_style(window.remaining_percent),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((screen.scroll, 0))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    let Some(screen) = state.usage_screen.as_ref() else {
        return;
    };
    let Some(account) = screen.selected_account() else {
        if screen.accounts.is_empty() {
            frame.render_widget(
                Paragraph::new("No providers configured.\n\nPress R to refresh.")
                    .block(panel("Overview"))
                    .wrap(Wrap { trim: false }),
                area,
            );
            return;
        }
        let mut lines = vec![Line::from("Status    available"), Line::from("")];
        for account in &screen.accounts {
            lines.push(Line::from(Span::styled(
                format!("{} · {}", account.provider, account.account),
                Style::default().fg(Color::White),
            )));
            if let Some(window) = account.windows.first() {
                let width = area.width.saturating_sub(8).max(8) as usize;
                lines.push(Line::from(Span::styled(
                    meter_line(width, window.remaining_percent),
                    meter_style(window.remaining_percent),
                )));
                lines.push(Line::from(format!("  {} · {}", window.value, window.reset)));
            } else {
                lines.push(Line::from(format!("  {}", account.status)));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(panel("Overview"))
                .wrap(Wrap { trim: false }),
            area,
        );
        return;
    };
    let title = if screen.detail { "Account" } else { "Overview" };
    let mut lines = vec![
        Line::from(format!("Provider  {}", account.provider)),
        Line::from(format!("Account   {}", account.account)),
        Line::from(format!("Status    {}", account.status)),
        Line::from(""),
        Line::from("Limits"),
    ];
    for window in &account.windows {
        lines.push(Line::from(Span::styled(
            format!("  {}", window.label),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
        let width = area.width.saturating_sub(8).max(8) as usize;
        lines.push(Line::from(Span::styled(
            meter_line(width, window.remaining_percent),
            meter_style(window.remaining_percent),
        )));
        let detail = if window.reset.is_empty() {
            format!("  {}", window.value)
        } else {
            format!("  {} · {}", window.value, window.reset)
        };
        lines.push(Line::from(detail));
        lines.push(Line::from(""));
    }
    if let Some(notice) = &screen.notice {
        lines.push(Line::from(Span::styled(
            notice.clone(),
            Style::default().fg(Color::Yellow),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(panel(title))
            .scroll((screen.scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
}

fn row_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    }
}

fn meter_line(width: usize, remaining: Option<u8>) -> String {
    let percent = usize::from(remaining.unwrap_or(0).min(100));
    let filled = width.saturating_mul(percent) / 100;
    format!(
        "  {}{}",
        "█".repeat(filled),
        "░".repeat(width.saturating_sub(filled))
    )
}

fn meter_style(remaining: Option<u8>) -> Style {
    match remaining.unwrap_or(0) {
        0..=15 => Style::default().fg(Color::Red),
        16..=35 => Style::default().fg(Color::Yellow),
        _ => Style::default().fg(Color::Green),
    }
}

#[cfg(test)]
mod tests;
