//! Editor screen view helpers.

use super::model::{EditorTab, SecretsScopeTag};
use crate::mount_display::{MountDisplayRow, mount_path_width};
use crate::tui::components::editor_rows::action_row_style;
use crate::tui::components::mount_rows::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH, render_mount_header,
};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorScrollGeometry {
    pub active_mounts: bool,
    pub content_width: usize,
    pub content_height: usize,
    pub mounts_content_width: usize,
}

pub fn clamp_editor_scroll_for_frame(
    body: Rect,
    geometry: EditorScrollGeometry,
    tab_scroll_x: &mut u16,
    tab_scroll_y: &mut u16,
    mounts_scroll_x: &mut u16,
) {
    let viewport_w = jackin_tui::components::scrollable_panel::viewport_width(body);
    let viewport_h = jackin_tui::components::scrollable_panel::viewport_height(body);
    if geometry.active_mounts {
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            geometry.mounts_content_width,
            viewport_w,
            mounts_scroll_x,
        );
    } else {
        jackin_tui::components::scrollable_panel::clamp_scroll_offset(
            geometry.content_width,
            viewport_w,
            tab_scroll_x,
        );
    }
    jackin_tui::components::scrollable_panel::clamp_scroll_offset(
        geometry.content_height,
        viewport_h,
        tab_scroll_y,
    );
}

pub fn editor_body_area(area: Rect, footer_h: u16) -> Rect {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(5),
            Constraint::Length(footer_h),
        ])
        .split(area);
    chunks[2]
}

pub fn editor_row_width(label: &str, value: &str) -> usize {
    padded_width(&format!("  {label:15}{value}"))
}

#[must_use]
pub fn general_lines(
    cursor: usize,
    show_cursor: bool,
    name_value: &str,
    workdir_display: &str,
    keep_awake_enabled: bool,
    git_pull_on_entry: bool,
) -> Vec<Line<'static>> {
    let keep_awake_display = if keep_awake_enabled {
        "enabled (macOS only)"
    } else {
        "disabled"
    };
    let git_pull_display = if git_pull_on_entry {
        "enabled"
    } else {
        "disabled"
    };
    vec![
        render_editor_row(0, cursor, "Name", name_value, show_cursor),
        render_editor_row(1, cursor, "Working dir", workdir_display, show_cursor),
        render_editor_row(2, cursor, "Keep awake", keep_awake_display, show_cursor),
        render_editor_row(3, cursor, "Git pull", git_pull_display, show_cursor),
    ]
}

#[must_use]
pub fn mount_lines(
    rows: &[MountDisplayRow],
    cursor: usize,
    hovered_row: Option<usize>,
    show_cursor: bool,
) -> Vec<Line<'static>> {
    let path_w = mount_path_width(rows);
    let mut lines: Vec<Line> = vec![render_mount_header(path_w)];

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
    let sentinel_prefix = if sentinel_selected {
        "\u{25b8} "
    } else {
        "  "
    };
    if !rows.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        format!("{sentinel_prefix}+ Add mount"),
        action_row_style(sentinel_selected),
    )));

    lines
}

fn render_editor_row(
    row: usize,
    cursor: usize,
    label: &str,
    value: &str,
    show_cursor: bool,
) -> Line<'static> {
    let selected = show_cursor && (row == cursor);
    let prefix = if selected { "\u{25b8} " } else { "  " };
    let label_style = if selected {
        Style::default()
            .fg(jackin_tui::theme::WHITE)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(jackin_tui::theme::WHITE)
    };
    let value_style = if selected {
        Style::default()
            .fg(jackin_tui::theme::PHOSPHOR_GREEN)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(jackin_tui::theme::PHOSPHOR_GREEN)
    };
    Line::from(vec![
        Span::styled(format!("{prefix}{label:15}"), label_style),
        Span::styled(value.to_string(), value_style),
    ])
}

pub fn padded_width(text: &str) -> usize {
    padded_width_cols(
        text_width(text),
        text.chars().take_while(|c| *c == ' ').count(),
    )
}

pub fn padded_width_cols(width: usize, leading_spaces: usize) -> usize {
    width + leading_spaces
}

pub fn text_width(text: &str) -> usize {
    jackin_tui::display_cols(text)
}

#[must_use]
pub fn tab_labels(active: EditorTab) -> Vec<(&'static str, bool)> {
    EditorTab::ALL
        .iter()
        .map(|tab| (tab.label(), *tab == active))
        .collect()
}

#[must_use]
pub fn secrets_scope_label(scope: &SecretsScopeTag) -> &str {
    match scope {
        SecretsScopeTag::Workspace => "workspace",
        SecretsScopeTag::Role(role) => role.as_str(),
    }
}

#[must_use]
pub fn secrets_forbidden_label(scope: &SecretsScopeTag) -> String {
    match scope {
        SecretsScopeTag::Workspace => "workspace env".to_string(),
        SecretsScopeTag::Role(role) => format!("role {role}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_lines_highlight_selected_row() {
        let lines = general_lines(2, true, "demo", "~/repo", true, false);

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].spans[0].content.as_ref(), "  Name           ");
        assert_eq!(lines[2].spans[0].content.as_ref(), "\u{25b8} Keep awake     ");
        assert_eq!(lines[2].spans[1].content.as_ref(), "enabled (macOS only)");
        assert_eq!(lines[3].spans[1].content.as_ref(), "disabled");
    }

    #[test]
    fn mount_lines_render_header_rows_and_sentinel() {
        let rows = [MountDisplayRow {
            destination: "/workspace".to_string(),
            host_source: Some("host: ~/project".to_string()),
            mode: "rw",
            isolation: "shared",
            kind: "bind".to_string(),
        }];

        let lines = mount_lines(&rows, 1, Some(0), true);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  Destination      Mode  Isolation  Type");
        assert_eq!(lines[1].spans[0].content.as_ref(), "  /workspace       ");
        assert_eq!(lines[2].spans[0].content.as_ref(), "  host: ~/project");
        assert_eq!(lines[4].spans[0].content.as_ref(), "\u{25b8} + Add mount");
    }
}
