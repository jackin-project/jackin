//! Mount row display data and width math shared by update and render.

use crate::tui::components::mount_rows::{MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH};

/// Pre-formatted mount row. `host_source` is `Some` only when src != dst.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountDisplayRow {
    pub destination: String,
    pub host_source: Option<String>,
    pub mode: &'static str,
    pub isolation: &'static str,
    pub kind: String,
}

/// Width of the `Destination` column, sized to fit the widest path plus header.
#[must_use]
pub fn mount_path_width(rows: &[MountDisplayRow]) -> usize {
    rows.iter()
        .flat_map(|row| std::iter::once(&row.destination).chain(row.host_source.as_ref()))
        .map(|p| jackin_tui::display_cols(p))
        .max()
        .unwrap_or(0)
        .max(10)
        .max("Destination".len())
}

#[must_use]
pub fn workspace_mounts_content_width(rows: &[MountDisplayRow]) -> usize {
    let path_w = mount_path_width(rows);
    rows.iter()
        .flat_map(|row| {
            [
                workspace_mount_row_width(path_w, row),
                row.host_source.as_ref().map_or(0, |_| 2 + path_w),
            ]
        })
        .chain([workspace_mount_header_width(path_w)])
        .max()
        .unwrap_or(0)
}

#[must_use]
pub fn global_mounts_content_width(rows: &[MountDisplayRow]) -> usize {
    let path_w = mount_path_width(rows);
    rows.iter()
        .flat_map(|row| {
            [
                global_mount_row_width(path_w),
                row.host_source.as_ref().map_or(0, |_| 2 + path_w),
            ]
        })
        .chain([global_mount_header_width(path_w)])
        .max()
        .unwrap_or(0)
}

#[must_use]
pub fn settings_global_mounts_content_width(rows: &[MountDisplayRow]) -> usize {
    let path_w = mount_path_width(rows);
    rows.iter()
        .flat_map(|row| {
            [
                settings_global_mount_row_width(path_w, row),
                row.host_source.as_ref().map_or(0, |_| 2 + path_w),
            ]
        })
        .chain((!rows.is_empty()).then_some(settings_global_mount_header_width(path_w)))
        .max()
        .unwrap_or(0)
}

fn workspace_mount_header_width(path_w: usize) -> usize {
    2 + path_w + 2 + MOUNT_MODE_COL_WIDTH + 2 + MOUNT_ISOLATION_COL_WIDTH + 2 + "Type".len()
}

fn workspace_mount_row_width(path_w: usize, row: &MountDisplayRow) -> usize {
    2 + path_w
        + 2
        + MOUNT_MODE_COL_WIDTH
        + 2
        + MOUNT_ISOLATION_COL_WIDTH
        + 2
        + jackin_tui::display_cols(&row.kind)
}

fn global_mount_header_width(path_w: usize) -> usize {
    2 + path_w + 2 + "Mode".len()
}

fn global_mount_row_width(path_w: usize) -> usize {
    2 + path_w + 2 + 2
}

fn settings_global_mount_header_width(path_w: usize) -> usize {
    2 + path_w + 2 + MOUNT_MODE_COL_WIDTH + 2 + "Type".len()
}

fn settings_global_mount_row_width(path_w: usize, row: &MountDisplayRow) -> usize {
    2 + path_w + 2 + MOUNT_MODE_COL_WIDTH + 2 + jackin_tui::display_cols(&row.kind)
}
