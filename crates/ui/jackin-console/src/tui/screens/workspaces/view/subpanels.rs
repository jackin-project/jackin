// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace env and mount subpanels.

use super::{env_row_line, panel};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::tui::mount_display::MountDisplayRow;

pub fn render_general_subpanel(frame: &mut Frame<'_>, area: Rect, workdir_display: &str) {
    let theme = termrock::style::DesignSystem::default();
    let lines = vec![Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "Working dir ",
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Text)
                .fg
                .unwrap_or_default()),
        ),
        Span::raw(workdir_display.to_owned()),
    ])];
    // Same padded viewport as the sibling subpanels so every block's content
    // column agrees (SUBPANEL_CONTENT_INDENT from the Panel body column).
    let mut scroll = termrock::scroll::DialogScroll::default();
    frame.render_stateful_widget(
        termrock::widgets::Viewport::new(&lines, &theme)
            .title("General")
            .padded_content()
            .content_style(
                Style::default().fg(theme
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default()),
            ),
        area,
        &mut scroll,
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEnvRow {
    pub name: String,
    pub scope: Option<String>,
    pub is_op: bool,
}

#[must_use]
pub fn workspace_env_rows(
    ws_config: Option<&jackin_config::WorkspaceConfig>,
) -> Vec<WorkspaceEnvRow> {
    let mut rows = Vec::new();
    if let Some(ws) = ws_config {
        for (key, value) in &ws.env {
            if jackin_core::is_account_env(key) {
                continue;
            }
            rows.push(WorkspaceEnvRow {
                name: key.clone(),
                scope: None,
                is_op: matches!(value, jackin_config::EnvValue::OpRef(_)),
            });
        }
        for (role, overrides) in &ws.roles {
            for (key, value) in &overrides.env {
                if jackin_core::is_account_env(key) {
                    continue;
                }
                rows.push(WorkspaceEnvRow {
                    name: key.clone(),
                    scope: Some(role.clone()),
                    is_op: matches!(value, jackin_config::EnvValue::OpRef(_)),
                });
            }
        }
    }
    rows
}

pub fn render_environments_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    mut rows: Vec<WorkspaceEnvRow>,
) {
    let theme = termrock::style::DesignSystem::default();
    let block = panel(&theme, Some(" Environments "), false).block();

    rows.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| match (&a.scope, &b.scope) {
                (None, None) => std::cmp::Ordering::Equal,
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) => x.cmp(y),
            })
    });

    let inner_width = termrock::scroll::viewport_width(area);
    let lines: Vec<Line<'_>> = rows
        .iter()
        .map(|row| env_row_line(row, inner_width))
        .collect();

    let panel = Paragraph::new(lines).block(block).style(
        Style::default().fg(termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default()),
    );
    frame.render_widget(panel, area);
}

pub fn render_mounts_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    rows: &[MountDisplayRow],
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        crate::tui::mount_display::workspace_mount_block_lines(rows),
        scroll_x,
        scroll_y,
        focused,
        Some(" Mounts "),
    );
}

pub fn render_config_mounts_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    mounts: &[jackin_config::MountConfig],
    cache: &crate::mount_info_cache::MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let rows = crate::tui::mount_display::format_config_mount_rows_with_cache(mounts, cache);
    render_mounts_subpanel(frame, area, &rows, scroll_x, scroll_y, focused);
}

pub fn render_global_mounts_subpanel(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: &[MountDisplayRow],
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    crate::tui::scroll_block::render_scrollable_block_at(
        frame,
        area,
        crate::tui::mount_display::global_mount_block_lines(rows),
        scroll_x,
        scroll_y,
        focused,
        Some(title),
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub fn render_global_mount_rows_section(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    rows: &[&jackin_config::GlobalMountRow],
    cache: &crate::mount_info_cache::MountInfoCache,
    scroll_x: u16,
    scroll_y: u16,
    focused: bool,
) {
    let mounts: Vec<jackin_config::MountConfig> =
        rows.iter().map(|row| row.mount.clone()).collect();
    let display_rows =
        crate::tui::mount_display::format_config_mount_rows_with_cache(&mounts, cache);
    render_global_mounts_subpanel(
        frame,
        area,
        title,
        &display_rows,
        scroll_x,
        scroll_y,
        focused,
    );
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRoleRow {
    pub name: String,
    pub exists: bool,
    pub is_default: bool,
    pub scoped_mount_count: usize,
}
