// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace roles subpanels.

use super::WorkspaceRoleRow;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
};

pub fn render_roles_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    default_role: Option<&str>,
    rows: Vec<WorkspaceRoleRow>,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mut lines: Vec<Line<'_>> = Vec::new();
    let (value_text, value_style): (String, Style) = default_role.map_or_else(
        || {
            (
                "(none)".to_owned(),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            )
        },
        |name| {
            (
                name.to_owned(),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default()),
            )
        },
    );
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Default ",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
        ),
        Span::styled(value_text, value_style),
    ]));
    lines.push(Line::from(""));

    for row in rows {
        let name_style = if row.exists {
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
        } else {
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default())
        };
        let mut spans = vec![Span::styled(format!("  {}", row.name), name_style)];
        if row.is_default {
            spans.push(Span::styled(
                " \u{2605}",
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            ));
        }
        if row.scoped_mount_count > 0 {
            spans.push(Span::styled(
                format!("    +{} role mounts", row.scoped_mount_count),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            ));
        }
        lines.push(Line::from(spans));
    }

    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        lines,
        scroll_x,
        scroll_y,
        focused,
        Some(" Roles "),
    );
}

pub fn render_config_roles_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    ws_config: Option<&jackin_config::WorkspaceConfig>,
    config: &jackin_config::AppConfig,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let allowed = ws_config.map_or(&[][..], |w| w.allowed_roles.as_slice());
    let all_allowed = ws_config.is_none_or(crate::workspace::allows_all_agents);
    let default = ws_config.and_then(|w| w.default_role.as_deref());

    let agent_names: Vec<&str> = if all_allowed {
        config.roles.keys().map(String::as_str).collect()
    } else {
        allowed.iter().map(String::as_str).collect()
    };
    let rows = agent_names
        .into_iter()
        .map(|role| WorkspaceRoleRow {
            name: role.to_owned(),
            exists: config.roles.contains_key(role),
            is_default: Some(role) == default,
            scoped_mount_count: role_scoped_mount_count(config, role),
        })
        .collect();
    render_roles_subpanel(frame, area, default, rows, scroll_x, scroll_y, focused);
}

pub(crate) fn role_scoped_mount_count(config: &jackin_config::AppConfig, role: &str) -> usize {
    jackin_core::RoleSelector::parse(role).map_or(0, |selector| {
        config
            .resolve_mount_rows(&selector)
            .into_iter()
            .filter(|row| row.scope.is_some())
            .count()
    })
}
