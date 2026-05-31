//! Shared mount-table row render helpers.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::console::manager::mount_display::{MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH};
use crate::console::widgets::WHITE;

pub(crate) fn render_mount_header(path_w: usize) -> Line<'static> {
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
