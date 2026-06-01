//! Shared mount-table row render helpers.

use crate::mount_display::MountDisplayRow;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use jackin_tui::theme::{PHOSPHOR_DIM, WHITE};

/// "Mode" header is 4 chars; pad row values so the Type column stays aligned.
pub const MOUNT_MODE_COL_WIDTH: usize = 4;

/// Width of the `Isolation` column, pinned to the widest known value/header.
pub const MOUNT_ISOLATION_COL_WIDTH: usize = 9;

#[must_use]
pub fn render_mount_header(path_w: usize) -> Line<'static> {
    let mode_col = format!("{:<mw$}", "Mode", mw = MOUNT_MODE_COL_WIDTH);
    let iso_col = format!("{:<iw$}", "Isolation", iw = MOUNT_ISOLATION_COL_WIDTH);
    Line::from(Span::styled(
        format!(
            "  {path:<path_w$}  {mode_col}  {iso_col}  Type",
            path = "Destination"
        ),
        Style::default().fg(WHITE),
    ))
}

#[must_use]
pub fn render_mount_lines(rows: &[MountDisplayRow], path_w: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<path_w$}  ", row.destination)),
            Span::styled(
                format!("{:<MOUNT_MODE_COL_WIDTH$}", row.mode),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{:<MOUNT_ISOLATION_COL_WIDTH$}", row.isolation),
                Style::default().fg(PHOSPHOR_DIM),
            ),
            Span::raw("  "),
            Span::styled(
                row.kind.clone(),
                Style::default()
                    .fg(PHOSPHOR_DIM)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(PHOSPHOR_DIM),
            )));
        }
    }
    lines
}

#[must_use]
pub fn render_global_mount_header(path_w: usize) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {path:<path_w$}  Mode", path = "Destination"),
        Style::default().fg(WHITE),
    ))
}

#[must_use]
pub fn render_global_mount_lines(rows: &[MountDisplayRow], path_w: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for row in rows {
        lines.push(Line::from(vec![
            Span::raw(format!("  {:<path_w$}  ", row.destination)),
            Span::styled(row.mode.to_string(), Style::default().fg(PHOSPHOR_DIM)),
        ]));
        if let Some(host_source) = &row.host_source {
            lines.push(Line::from(Span::styled(
                format!("  {host_source:<path_w$}"),
                Style::default().fg(PHOSPHOR_DIM),
            )));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(destination: &str, host_source: Option<&str>) -> MountDisplayRow {
        MountDisplayRow {
            destination: destination.to_string(),
            host_source: host_source.map(str::to_string),
            mode: "rw",
            isolation: "shared",
            kind: "bind".to_string(),
        }
    }

    #[test]
    fn mount_lines_render_rows_and_sources() {
        let rows = [row("/workspace", Some("host: ~/repo"))];
        let lines = render_mount_lines(&rows, 12);

        assert_eq!(lines[0].spans[0].content.as_ref(), "  /workspace    ");
        assert_eq!(lines[0].spans[1].content.as_ref(), "rw  ");
        assert_eq!(lines[0].spans[3].content.as_ref(), "shared   ");
        assert_eq!(lines[1].spans[0].content.as_ref(), "  host: ~/repo");
    }

    #[test]
    fn global_mount_lines_render_header_and_rows() {
        let rows = [row("/cache", None)];
        let header = render_global_mount_header(12);
        let lines = render_global_mount_lines(&rows, 12);

        assert_eq!(header.spans[0].content.as_ref(), "  Destination   Mode");
        assert_eq!(lines[0].spans[0].content.as_ref(), "  /cache        ");
        assert_eq!(lines[0].spans[1].content.as_ref(), "rw");
    }
}
