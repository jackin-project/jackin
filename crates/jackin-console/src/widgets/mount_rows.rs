//! Shared mount-table row render helpers.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use super::WHITE;

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
