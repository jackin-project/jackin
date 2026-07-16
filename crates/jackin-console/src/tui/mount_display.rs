// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Mount row display data and the lines rendered for each mount block.
//!
//! Scroll extents are derived from the rendered lines, never recomputed: a
//! block's `content_width` is `max_line_width(&<block>_lines(rows))` and its
//! height is the line count. The input-side scroll clamp consumes the same
//! values, so the clamp can never disagree with what the renderer draws — the
//! disagreement that made the horizontal scrollbar thumb stop short of the end.

use ratatui::text::{Line, Span};

use termrock::scroll::max_line_width;

use crate::mount_info_cache::MountInfoCache;
use crate::tui::components::mount_rows::{
    render_global_mount_header, render_global_mount_lines, render_mount_header, render_mount_lines,
};

/// Pre-formatted mount row. `host_source` is `Some` only when src != dst.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountDisplayRow {
    pub destination: String,
    pub host_source: Option<String>,
    pub mode: &'static str,
    pub isolation: &'static str,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountDisplayInput<'a> {
    pub src: &'a str,
    pub dst: &'a str,
    pub readonly: bool,
    pub isolation: &'static str,
    pub kind: String,
}

pub fn mount_display_paths(
    src: &str,
    dst: &str,
    shorten: impl Fn(&str) -> String,
) -> (String, Option<String>) {
    let src_display = shorten(src);
    let dst_display = shorten(dst);
    if src == dst {
        (dst_display, None)
    } else {
        (dst_display, Some(format!("host: {src_display}")))
    }
}

pub fn format_mount_rows<'a>(
    mounts: impl IntoIterator<Item = MountDisplayInput<'a>> + 'a,
    shorten: impl Fn(&str) -> String + Copy,
) -> Vec<MountDisplayRow> {
    mounts
        .into_iter()
        .map(|mount| {
            let (destination, host_source) = mount_display_paths(mount.src, mount.dst, shorten);
            MountDisplayRow {
                destination,
                host_source,
                mode: if mount.readonly { "ro" } else { "rw" },
                isolation: mount.isolation,
                kind: mount.kind,
            }
        })
        .collect()
}

pub fn format_config_mount_rows_with_cache(
    mounts: &[jackin_config::MountConfig],
    cache: &MountInfoCache,
) -> Vec<MountDisplayRow> {
    format_mount_rows(
        mounts.iter().map(|m| MountDisplayInput {
            src: &m.src,
            dst: &m.dst,
            readonly: m.readonly,
            isolation: m.isolation.as_str(),
            kind: cache.label(&m.src),
        }),
        jackin_core::shorten_home,
    )
}

#[cfg(test)]
pub fn format_config_mount_rows(mounts: &[jackin_config::MountConfig]) -> Vec<MountDisplayRow> {
    let cache = MountInfoCache::default();
    cache.refresh_mounts(mounts);
    format_config_mount_rows_with_cache(mounts, &cache)
}

pub fn workspace_config_mounts_content_width_with_cache(
    mounts: &[jackin_config::MountConfig],
    cache: &MountInfoCache,
) -> usize {
    let rows = format_config_mount_rows_with_cache(mounts, cache);
    workspace_mounts_content_width(&rows)
}

#[cfg(test)]
pub fn workspace_config_mounts_content_width(mounts: &[jackin_config::MountConfig]) -> usize {
    let cache = MountInfoCache::default();
    cache.refresh_mounts(mounts);
    workspace_config_mounts_content_width_with_cache(mounts, &cache)
}

pub fn workspace_config_mounts_content_height(mounts: &[jackin_config::MountConfig]) -> usize {
    mounts_content_height(mounts.iter().map(|m| m.src == m.dst))
}

pub fn global_config_mounts_content_width_with_cache(
    mounts: &[jackin_config::MountConfig],
    cache: &MountInfoCache,
) -> usize {
    let rows = format_config_mount_rows_with_cache(mounts, cache);
    global_mounts_content_width(&rows)
}

#[cfg(test)]
pub fn global_config_mounts_content_width(mounts: &[jackin_config::MountConfig]) -> usize {
    let cache = MountInfoCache::default();
    cache.refresh_mounts(mounts);
    global_config_mounts_content_width_with_cache(mounts, &cache)
}

pub fn settings_global_config_mounts_content_width_with_cache(
    rows: &[jackin_config::GlobalMountRow],
    cache: &MountInfoCache,
) -> usize {
    let mounts = rows.iter().map(|row| row.mount.clone()).collect::<Vec<_>>();
    let display_rows = format_config_mount_rows_with_cache(&mounts, cache);
    // Width comes from the exact lines the settings tab renders, so the scroll
    // clamp agrees with the renderer. Selection is width-invariant (the `> `
    // and `  ` prefixes are both 2 cols), so building without a selection is
    // safe; the sentinel row is included to match the rendered block.
    let lines = crate::tui::screens::settings::view::global_mount_lines(&display_rows, None, true);
    max_line_width(&lines)
}

pub fn settings_global_config_mounts_content_height(
    rows: &[jackin_config::GlobalMountRow],
) -> usize {
    settings_global_mounts_content_height(
        rows.iter().map(|row| row.mount.src == row.mount.dst),
        rows.is_empty(),
    )
}

/// Width of the `Destination` column, sized to fit the widest path plus header.
#[must_use]
pub fn mount_path_width(rows: &[MountDisplayRow]) -> usize {
    rows.iter()
        .flat_map(|row| std::iter::once(&row.destination).chain(row.host_source.as_ref()))
        .map(|p| termrock::display_cols(p))
        .max()
        .unwrap_or(0)
        .max(10)
        .max("Destination".len())
}

/// Dim `(none)` placeholder shown when a read-only mount block has no rows.
fn none_placeholder_line() -> Line<'static> {
    Line::from(Span::styled(
        "  (none)",
        ratatui::style::Style::default().fg(termrock::style::PHOSPHOR_DIM),
    ))
}

/// Canonical lines for the read-only **Mounts** block. The single source both
/// the renderer and the scroll-extent math consume — see the module docs.
#[must_use]
pub fn workspace_mount_block_lines(rows: &[MountDisplayRow]) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return vec![
            render_mount_header(mount_path_width(&[])),
            none_placeholder_line(),
        ];
    }
    let path_w = mount_path_width(rows);
    let mut lines = Vec::with_capacity(rows.len() * 2 + 1);
    lines.push(render_mount_header(path_w));
    lines.extend(render_mount_lines(rows, path_w));
    lines
}

/// Canonical lines for a read-only **Global mounts** / role-global block.
#[must_use]
pub fn global_mount_block_lines(rows: &[MountDisplayRow]) -> Vec<Line<'static>> {
    if rows.is_empty() {
        return vec![none_placeholder_line()];
    }
    let path_w = mount_path_width(rows);
    let mut lines = Vec::with_capacity(rows.len() * 2 + 1);
    lines.push(render_global_mount_header(path_w));
    lines.extend(render_global_mount_lines(rows, path_w));
    lines
}

#[must_use]
pub fn workspace_mounts_content_width(rows: &[MountDisplayRow]) -> usize {
    max_line_width(&workspace_mount_block_lines(rows))
}

#[must_use]
pub fn global_mounts_content_width(rows: &[MountDisplayRow]) -> usize {
    max_line_width(&global_mount_block_lines(rows))
}

#[must_use]
pub fn mounts_content_height(same_path_rows: impl IntoIterator<Item = bool>) -> usize {
    1 + same_path_rows
        .into_iter()
        .map(|same_path| if same_path { 1 } else { 2 })
        .sum::<usize>()
        .max(1)
}

#[must_use]
pub fn settings_global_mounts_content_height(
    same_path_rows: impl IntoIterator<Item = bool>,
    is_empty: bool,
) -> usize {
    if is_empty {
        return 1;
    }
    same_path_rows
        .into_iter()
        .map(|same_path| if same_path { 1 } else { 2 })
        .sum::<usize>()
        + 3
}

#[cfg(test)]
mod tests;
