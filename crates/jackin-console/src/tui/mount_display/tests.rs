// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use termrock::display_cols;

fn wide_row() -> MountDisplayRow {
    MountDisplayRow {
        destination: "/home/agent/projects/jackin-project/jackin".into(),
        host_source: Some("host: ~/.cache/jackin/global/cargo/registry".into()),
        mode: "rw",
        isolation: "shared",
        kind: "github · feature/tui-architecture".into(),
    }
}

fn config_mount() -> jackin_config::MountConfig {
    jackin_config::MountConfig {
        src: "/host/project".to_owned(),
        dst: "/workspace".to_owned(),
        readonly: true,
        isolation: jackin_config::MountIsolation::Shared,
    }
}

fn widest_unpadded(lines: &[Line<'_>]) -> usize {
    lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| display_cols(&s.content))
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0)
}

#[test]
fn workspace_width_equals_rendered_line_width() {
    // Single-source invariant: the scroll-clamp width is exactly the width
    // the renderer measures over the same lines. If a parallel column-sum
    // is ever reintroduced, this catches the drift.
    let rows = vec![wide_row()];
    assert_eq!(
        workspace_mounts_content_width(&rows),
        max_line_width(&workspace_mount_block_lines(&rows)),
    );
}

#[test]
fn workspace_width_includes_trailing_scroll_pad() {
    // Mount lines carry a 2-space indent that render_scrollable_block mirrors
    // as 2 trailing pad columns. content_width MUST include that pad, or the
    // horizontal scrollbar thumb stops 2 cells short of the end.
    let rows = vec![wide_row()];
    let lines = workspace_mount_block_lines(&rows);
    assert_eq!(
        workspace_mounts_content_width(&rows),
        widest_unpadded(&lines) + 2,
    );
}

#[test]
fn global_width_equals_rendered_line_width() {
    let rows = vec![wide_row()];
    assert_eq!(
        global_mounts_content_width(&rows),
        max_line_width(&global_mount_block_lines(&rows)),
    );
}

#[test]
fn config_mount_rows_use_cache_kind_and_display_paths() {
    let cache = MountInfoCache::default();
    let mounts = vec![config_mount()];

    let rows = format_config_mount_rows_with_cache(&mounts, &cache);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].destination, "/workspace");
    assert_eq!(rows[0].host_source, Some("host: /host/project".to_owned()));
    assert_eq!(rows[0].mode, "ro");
    assert_eq!(rows[0].isolation, "shared");
    assert_eq!(rows[0].kind, "unknown");
    let refreshed_rows = format_config_mount_rows(&mounts);
    assert_eq!(refreshed_rows[0].kind, "missing");
}
