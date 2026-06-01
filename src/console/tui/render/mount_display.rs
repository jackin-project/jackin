//! Mount row display data and width math shared by update and render.

use crate::console::tui::state::MountInfoCache;

pub(crate) use jackin_console::mount_display::{MountDisplayRow, mount_path_width};
pub(crate) use jackin_console::tui::components::mount_rows::{
    MOUNT_ISOLATION_COL_WIDTH, MOUNT_MODE_COL_WIDTH,
};

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
    jackin_console::mount_display::workspace_mounts_content_width(&rows)
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
    jackin_console::mount_display::global_mounts_content_width(&rows)
}

pub(crate) fn settings_global_mounts_content_width_with_cache(
    rows: &[crate::config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    jackin_console::mount_display::settings_global_mounts_content_width(&display_rows)
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
