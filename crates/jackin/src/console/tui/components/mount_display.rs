//! Mount row display data and width math shared by update and render.

use crate::console::tui::state::MountInfoCache;

use jackin_console::tui::mount_display::MountDisplayInput;
pub(crate) use jackin_console::tui::mount_display::MountDisplayRow;
#[cfg(test)]
pub(crate) use jackin_console::tui::mount_display::mount_path_width;

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
    jackin_console::tui::mount_display::format_mount_rows(
        mounts.iter().map(|m| MountDisplayInput {
            src: &m.src,
            dst: &m.dst,
            readonly: m.readonly,
            isolation: m.isolation.as_str(),
            kind: cache.label(&m.src),
        }),
        crate::tui::shorten_home,
    )
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
    jackin_console::tui::mount_display::workspace_mounts_content_width(&rows)
}

pub(crate) fn workspace_mounts_content_height(mounts: &[crate::workspace::MountConfig]) -> usize {
    jackin_console::tui::mount_display::mounts_content_height(mounts.iter().map(|m| m.src == m.dst))
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
    jackin_console::tui::mount_display::global_mounts_content_width(&rows)
}

pub(crate) fn settings_global_mounts_content_width_with_cache(
    rows: &[crate::config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_mount_rows_with_cache(&mounts, cache);
    jackin_console::tui::mount_display::settings_global_mounts_content_width(&display_rows)
}

pub(crate) fn settings_global_mounts_content_height(
    rows: &[crate::config::GlobalMountRow],
) -> usize {
    jackin_console::tui::mount_display::settings_global_mounts_content_height(
        rows.iter().map(|row| row.mount.src == row.mount.dst),
        rows.is_empty(),
    )
}
