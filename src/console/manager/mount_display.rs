//! Mount row display data and width math shared by update and render.

use crate::console::manager::state::MountInfoCache;

/// Pre-formatted mount row. `host_source` is `Some` only when src != dst.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MountDisplayRow {
    pub(crate) destination: String,
    pub(crate) host_source: Option<String>,
    pub(crate) mode: &'static str,
    pub(crate) isolation: &'static str,
    pub(crate) kind: String,
}

pub(crate) fn mount_display_paths(
    mount: &crate::workspace::MountConfig,
) -> (String, Option<String>) {
    let src = crate::tui::shorten_home(&mount.src);
    let dst = crate::tui::shorten_home(&mount.dst);
    if mount.src == mount.dst {
        (dst, None)
    } else {
        (dst, Some(format!("host: {src}")))
    }
}

#[cfg(test)]
pub(crate) fn format_mount_rows(mounts: &[crate::workspace::MountConfig]) -> Vec<MountDisplayRow> {
    let cache = MountInfoCache::default();
    cache.refresh_mounts(mounts);
    format_mount_rows_with_cache(mounts, &cache)
}

pub(crate) fn format_mount_rows_with_cache(
    mounts: &[crate::workspace::MountConfig],
    cache: &MountInfoCache,
) -> Vec<MountDisplayRow> {
    mounts
        .iter()
        .map(|m| {
            let (destination, host_source) = mount_display_paths(m);
            let mode: &'static str = if m.readonly { "ro" } else { "rw" };
            let isolation: &'static str = m.isolation.as_str();
            let kind = cache.label(&m.src);
            MountDisplayRow {
                destination,
                host_source,
                mode,
                isolation,
                kind,
            }
        })
        .collect()
}

/// "Mode" header is 4 chars; pad row values so the Type column stays aligned.
pub(crate) const MOUNT_MODE_COL_WIDTH: usize = 4;

/// Width of the `Isolation` column, pinned to the widest known value/header.
pub(crate) const MOUNT_ISOLATION_COL_WIDTH: usize = 9;

/// Width of the `Destination` column, sized to fit the widest path plus header.
pub(crate) fn mount_path_width(rows: &[MountDisplayRow]) -> usize {
    rows.iter()
        .flat_map(|row| std::iter::once(&row.destination).chain(row.host_source.as_ref()))
        .map(|p| jackin_tui::display_cols(p))
        .max()
        .unwrap_or(0)
        .max(10)
        .max("Destination".len())
}

#[cfg(test)]
pub(crate) fn workspace_mounts_content_width(mounts: &[crate::workspace::MountConfig]) -> usize {
    let cache = MountInfoCache::default();
    cache.refresh_mounts(mounts);
    workspace_mounts_content_width_with_cache(mounts, &cache)
}

pub(crate) fn workspace_mounts_content_width_with_cache(
    mounts: &[crate::workspace::MountConfig],
    cache: &MountInfoCache,
) -> usize {
    let rows = format_mount_rows_with_cache(mounts, cache);
    let path_w = mount_path_width(&rows);
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

pub(crate) fn workspace_mounts_content_height(mounts: &[crate::workspace::MountConfig]) -> usize {
    1 + mounts
        .iter()
        .map(|m| if m.src == m.dst { 1 } else { 2 })
        .sum::<usize>()
        .max(1)
}

#[cfg(test)]
pub(crate) fn global_mounts_content_width(mounts: &[crate::workspace::MountConfig]) -> usize {
    let cache = MountInfoCache::default();
    cache.refresh_mounts(mounts);
    global_mounts_content_width_with_cache(mounts, &cache)
}

pub(crate) fn global_mounts_content_width_with_cache(
    mounts: &[crate::workspace::MountConfig],
    cache: &MountInfoCache,
) -> usize {
    let rows = format_mount_rows_with_cache(mounts, cache);
    let path_w = mount_path_width(&rows);
    rows.iter()
        .flat_map(|row| {
            [
                global_mount_row_width(path_w, row),
                row.host_source.as_ref().map_or(0, |_| 2 + path_w),
            ]
        })
        .chain([global_mount_header_width(path_w)])
        .max()
        .unwrap_or(0)
}

pub(crate) fn settings_global_mounts_content_width_with_cache(
    rows: &[crate::config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    let path_w = mount_path_width(&display_rows);
    display_rows
        .iter()
        .flat_map(|row| {
            [
                settings_global_mount_row_width(path_w, row),
                row.host_source.as_ref().map_or(0, |_| 2 + path_w),
            ]
        })
        .chain((!display_rows.is_empty()).then_some(settings_global_mount_header_width(path_w)))
        .max()
        .unwrap_or(0)
}

pub(crate) fn settings_global_mounts_content_height(
    rows: &[crate::config::GlobalMountRow],
) -> usize {
    if rows.is_empty() {
        return 1;
    }
    let row_lines = rows
        .iter()
        .map(|row| if row.mount.src == row.mount.dst { 1 } else { 2 })
        .sum::<usize>();
    row_lines + 3
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

fn global_mount_row_width(path_w: usize, _row: &MountDisplayRow) -> usize {
    2 + path_w + 2 + 2
}

fn settings_global_mount_header_width(path_w: usize) -> usize {
    2 + path_w + 2 + MOUNT_MODE_COL_WIDTH + 2 + "Type".len()
}

fn settings_global_mount_row_width(path_w: usize, row: &MountDisplayRow) -> usize {
    2 + path_w + 2 + MOUNT_MODE_COL_WIDTH + 2 + jackin_tui::display_cols(&row.kind)
}
