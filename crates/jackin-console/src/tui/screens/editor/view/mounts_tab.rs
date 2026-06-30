//! Mounts tab lines, geometry, and width helpers extracted from the view
//! coordinator. Items re-exported from parent to preserve call sites.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::components::editor_rows::action_row_style;
use crate::tui::components::mount_rows::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, render_mount_header,
};
use crate::tui::mount_display::{
    MountDisplayRow, format_config_mount_rows_with_cache, mount_path_width,
};

use super::WorkspaceEditorState;
use crate::tui::screens::editor::model::FieldFocus;

#[must_use]
pub(crate) fn editor_mount_add_row_width() -> usize {
    super::text_width("  + Add mount")
}

#[must_use]
#[allow(clippy::type_complexity)]
pub(crate) fn mount_state_geometry<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
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
) -> super::EditorTabContentGeometry {
    let content_height = if state.pending.mounts.is_empty() {
        2
    } else {
        crate::tui::mount_display::workspace_config_mounts_content_height(&state.pending.mounts) + 2
    };
    super::EditorTabContentGeometry {
        content_width: crate::tui::mount_display::workspace_config_mounts_content_width_with_cache(
            &state.pending.mounts,
            &state.mount_info_cache,
        )
        .max(editor_mount_add_row_width()),
        content_height,
    }
}

#[must_use]
#[allow(unreachable_pub)]
pub(crate) fn mount_lines(
    rows: &[MountDisplayRow],
    cursor: usize,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line<'_>> = vec![render_mount_header(path_w)];

    for (i, row) in rows.iter().enumerate() {
        let selected = show_cursor && (i == cursor);
        let hovered = !selected && hovered_row == Some(i);
        let hb = |s: Style| {
            if hovered {
                s.bg(jackin_tui::theme::TAB_BG_INACTIVE_HOVER)
            } else {
                s
            }
        };
        let prefix = if selected { "\u{25b8} " } else { "  " };
        let base_style = if selected {
            Style::default()
                .fg(jackin_tui::theme::PHOSPHOR_GREEN)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
        };
        let dim_style = Style::default()
            .fg(jackin_tui::theme::PHOSPHOR_DIM)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(vec![
            Span::styled(
                format!("{prefix}{:<path_w$}  ", row.destination),
                hb(base_style),
            ),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                hb(Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM)),
            ),
            Span::styled("  ", hb(Style::default())),
            Span::styled(
                format!("{:<MOUNT_ISOLATION_COL_WIDTH$}", row.isolation),
                hb(Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM)),
            ),
            Span::styled("  ", hb(Style::default())),
            Span::styled(row.kind.clone(), hb(dim_style)),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
            )));
        }
    }

    let sentinel_idx = rows.len();
    let sentinel_selected = show_cursor && (cursor == sentinel_idx);
    let sentinel_prefix = if sentinel_selected { "\u{25b8} " } else { "  " };
    if !rows.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Add mount"),
        action_row_style(sentinel_selected),
    )));

    lines
}

#[must_use]
#[allow(clippy::type_complexity)]
pub(crate) fn mount_state_lines<
    Modal,
    SaveFlow,
    EnvValue,
    AuthFormTarget,
    PendingTokenGenerate,
    PendingRoleLoad,
    PendingDriftCheck,
    PendingIsolationCleanup,
    PendingOpCommit,
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
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let FieldFocus::Row(cursor) = state.active_field;
    let rows = format_config_mount_rows_with_cache(&state.pending.mounts, &state.mount_info_cache);
    mount_lines(&rows, cursor, state.hovered_mount_row(), show_cursor)
}
