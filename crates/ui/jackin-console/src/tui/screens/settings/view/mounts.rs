// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Mounts tab lines.

use super::{render_settings_screen, settings_footer_items, settings_frame_areas};

use super::super::model::GlobalMountsState;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::tui::components::editor_rows::action_row_style;
use crate::tui::components::footer_hints::{
    SettingsScreenFooterFacts, settings_screen_footer_items,
};
use crate::tui::components::mount_rows::{MOUNT_MODE_COL_WIDTH, render_global_mount_header};
use crate::tui::mount_display::{
    MountDisplayRow, format_config_mount_rows_with_cache, mount_path_width,
};
use crate::tui::state::SettingsModal;
use termrock::widgets::HintSpan;

#[must_use]
pub fn global_mount_lines(
    rows: &[MountDisplayRow],
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line<'static>> = Vec::new();
    if !rows.is_empty() {
        lines.push(render_global_mount_header(path_w));
    }
    for (i, row) in rows.iter().enumerate() {
        let is_selected = selected == Some(i);
        let prefix = if is_selected { "\u{25b8} " } else { "  " };
        let base_style = if is_selected {
            Style::default()
                .fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::Accent)
                .fg
                .unwrap_or_default())
        };
        let dim_style = Style::default()
            .fg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TextMuted)
                .fg
                .unwrap_or_default())
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{prefix}{:<path_w$}  ", row.destination),
                base_style,
            ),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            ),
            Span::raw("  "),
            Span::styled(row.kind.clone(), dim_style),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TextMuted)
                    .fg
                    .unwrap_or_default()),
            )));
        }
    }
    if include_sentinel {
        let sentinel_selected = selected == Some(rows.len());
        let sentinel_prefix = if sentinel_selected { "\u{25b8} " } else { "  " };
        if !rows.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            format!("{sentinel_prefix}+ Add mount"),
            action_row_style(sentinel_selected),
        )));
    }
    lines
}

#[must_use]
pub fn global_mount_state_lines<Modal>(
    state: &GlobalMountsState<jackin_config::GlobalMountRow, Modal>,
    selected: Option<usize>,
    include_sentinel: bool,
) -> Vec<Line<'static>> {
    let mounts = state
        .pending
        .iter()
        .map(|row| row.mount.clone())
        .collect::<Vec<_>>();
    let display_rows = format_config_mount_rows_with_cache(&mounts, &state.mount_info_cache);
    global_mount_lines(&display_rows, selected, include_sentinel)
}

pub(crate) fn truncate(value: &str, width: usize) -> String {
    let mut out: String = value.chars().take(width).collect();
    if value.chars().count() > width && width > 1 {
        out.pop();
        out.push('\u{2026}');
    }
    out
}

pub fn clamp_mounts_scroll_x_for_frame(
    area: Rect,
    content_width: usize,
    scroll: &mut termrock::widgets::ScrollAreaState,
) {
    let areas = settings_frame_areas(area, 2);
    // Clamp the X axis only, exactly as the retired raw-offset helper did:
    // Y dims stay pinned so `clamp` can never touch the vertical offset.
    // The pinned viewport is 1, not 0 — upstream `max_offset(_, 0)` is 0,
    // which would clamp the vertical offset to zero.
    scroll.set_content_size(u16::try_from(content_width).unwrap_or(u16::MAX), u16::MAX);
    scroll.set_viewport(
        u16::try_from(termrock::scroll::viewport_width(areas.body)).unwrap_or(u16::MAX),
        1,
    );
    scroll.clamp();
}

/// Concrete adapter: render the settings screen for a concrete `SettingsState`.
pub fn render_settings_with_footer(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &crate::tui::state::SettingsState<'_>,
    op_available: bool,
) {
    render_settings_screen(frame, area, state, |state, body| {
        settings_screen_footer_for_state(state, op_available, body)
    });
}

/// Concrete adapter: compose settings footer items for a concrete `SettingsState`.
///
/// Gives modals priority over screen items, so whatever is active on-screen
/// gets the footer real-estate. The generic `settings_footer_items` handles
/// per-screen hint routing; this function layers modal items on top.
#[must_use]
pub fn settings_screen_footer_for_state(
    state: &crate::tui::state::SettingsState<'_>,
    op_available: bool,
    body_area: Rect,
) -> Vec<HintSpan<'static>> {
    settings_screen_footer_items(SettingsScreenFooterFacts {
        auth_modal_items: state
            .auth
            .modal_ref()
            .map(|modal| modal.auth_footer_items(false)),
        env_modal_items: state
            .env
            .modals
            .current()
            .map(SettingsModal::env_footer_items),
        mounts_modal_items: state
            .mounts
            .modals
            .current()
            .map(SettingsModal::mounts_footer_items),
        screen_items: settings_footer_items(state, op_available, body_area),
    })
}
