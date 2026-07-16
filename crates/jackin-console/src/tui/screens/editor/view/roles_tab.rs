// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Roles tab lines, geometry, widths, and `EditorRoleRow` extracted from the
//! view coordinator. All items re-exported from parent to preserve `super::`
//! call sites (e.g. in frame.rs via `render_roles_tab` and in tests via
//! `use super::*`).

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tui::components::editor_rows::action_row_style;
use crate::tui::screens::editor::model::FieldFocus;

use super::WorkspaceEditorState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditorRoleRow {
    pub name: String,
    pub effectively_allowed: bool,
    pub is_default: bool,
}

#[must_use]
pub(crate) fn editor_roles_status_width(
    is_all: bool,
    allowed_count: usize,
    total_count: usize,
) -> usize {
    if is_all {
        super::text_width("  Allowed roles:    all  ")
    } else {
        super::text_width(&format!(
            "  Allowed roles:    custom     ({allowed_count} of {total_count} allowed)"
        ))
    }
}

#[must_use]
pub(crate) fn editor_role_row_width(role_name: &str) -> usize {
    super::text_width(&format!("  [x] * {role_name}"))
}

#[must_use]
pub(crate) fn editor_role_load_row_width() -> usize {
    super::text_width("  + Load role")
}

#[must_use]
pub(crate) fn role_lines(
    rows: &[EditorRoleRow],
    allowed_count: usize,
    is_all: bool,
    cursor: usize,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let badge_text = if is_all { "  all  " } else { "  custom  " };
    let badge_bg = if is_all {
        jackin_ui::theme::accent_fg()
    } else {
        jackin_ui::theme::text_fg()
    };
    let badge_style = Style::default()
        .bg(badge_bg)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let mut status_spans = vec![
        Span::styled("  Allowed roles:  ", jackin_ui::theme::text_strong()),
        Span::styled(badge_text, badge_style),
    ];
    if !is_all {
        status_spans.push(Span::styled(
            format!("   ({allowed_count} of {} allowed)", rows.len()),
            Style::default()
                .fg(jackin_ui::theme::ACTION_ACCENT)
                .add_modifier(Modifier::ITALIC),
        ));
    }

    let mut lines = vec![Line::from(status_spans), Line::from("")];

    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (i == cursor);
        let check = if row.effectively_allowed {
            "[x]"
        } else {
            "[ ]"
        };
        let star = if row.is_default { "\u{2605}" } else { " " };
        let prefix = if selected { "\u{25b8} " } else { "  " };
        let text = format!("{prefix}{check} {star} {}", row.name);
        let style = if selected {
            Style::default()
                .fg(jackin_ui::theme::accent_fg())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(jackin_ui::theme::accent_fg())
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    let sentinel_idx = rows.len();
    let sentinel_selected = show_cursor && (cursor == sentinel_idx);
    let sentinel_prefix = if sentinel_selected { "\u{25b8} " } else { "  " };
    if !rows.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Load role"),
        action_row_style(sentinel_selected),
    )));

    lines
}

#[must_use]
pub(crate) fn role_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
    RoleName,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    role_names: impl IntoIterator<Item = RoleName>,
    show_cursor: bool,
) -> Vec<Line<'static>>
where
    RoleName: AsRef<str>,
{
    let FieldFocus::Row(cursor) = state.active_field;
    let is_all = crate::workspace::allows_all_agents(&state.pending);
    let allowed_count = state.pending.allowed_roles.len();
    let rows: Vec<EditorRoleRow> = role_names
        .into_iter()
        .map(|role_name| {
            let role_name = role_name.as_ref();
            EditorRoleRow {
                name: role_name.to_owned(),
                effectively_allowed: crate::workspace::agent_is_effectively_allowed(
                    &state.pending,
                    role_name,
                ),
                is_default: state.pending.default_role.as_deref() == Some(role_name),
            }
        })
        .collect();

    role_lines(&rows, allowed_count, is_all, cursor, show_cursor)
}

#[must_use]
pub(crate) fn role_state_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
    RoleName,
>(
    state: &WorkspaceEditorState<
        Modal,
        SaveFlow,
        EnvValue,
        AuthFormTarget,
        PendingTokenGenerate,
        PendingRoleLoad,
        PendingDriftCheck,
        PendingIsolationCleanup,
        PendingOpCommit,
    >,
    role_names: impl IntoIterator<Item = RoleName>,
) -> super::EditorTabContentGeometry
where
    RoleName: AsRef<str>,
{
    let role_names: Vec<String> = role_names
        .into_iter()
        .map(|role_name| role_name.as_ref().to_owned())
        .collect();
    let is_all = crate::workspace::allows_all_agents(&state.pending);
    let allowed_count = state.pending.allowed_roles.len();
    let total = role_names.len();
    let status_width = editor_roles_status_width(is_all, allowed_count, total);
    let role_width = role_names
        .iter()
        .map(|role_name| editor_role_row_width(role_name))
        .max()
        .unwrap_or(0);
    super::EditorTabContentGeometry {
        content_width: status_width
            .max(role_width)
            .max(editor_role_load_row_width()),
        content_height: 2 + total + usize::from(total > 0) + 1,
    }
}
