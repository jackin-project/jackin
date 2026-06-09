//! Mount row display data and the lines rendered for each mount block.
//!
//! Scroll extents are derived from the rendered lines, never recomputed: a
//! block's `content_width` is `max_line_width(&<block>_lines(rows))` and its
//! height is the line count. The input-side scroll clamp consumes the same
//! values, so the clamp can never disagree with what the renderer draws — the
//! disagreement that made the horizontal scrollbar thumb stop short of the end.

use ratatui::text::{Line, Span};

use jackin_tui::components::scrollable_panel::max_line_width;

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

#[allow(unfulfilled_lint_expectations)]
#[expect(
    single_use_lifetimes,
    reason = "impl Trait cannot use anonymous lifetimes for borrowed mount DTOs on stable Rust"
)]
pub fn format_mount_rows<'a>(
    mounts: impl IntoIterator<Item = MountDisplayInput<'a>>,
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

/// Dim `(none)` placeholder shown when a read-only mount block has no rows.
fn none_placeholder_line() -> Line<'static> {
    Line::from(Span::styled(
        "  (none)",
        ratatui::style::Style::default().fg(jackin_tui::theme::PHOSPHOR_DIM),
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
mod tests {
    use super::*;
    use jackin_tui::display_cols;

    fn wide_row() -> MountDisplayRow {
        MountDisplayRow {
            destination: "/home/agent/projects/jackin-project/jackin".into(),
            host_source: Some("host: ~/.cache/jackin/global/cargo/registry".into()),
            mode: "rw",
            isolation: "shared",
            kind: "github · feature/tui-architecture".into(),
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
        // horizontal scrollbar thumb stops 2 cells short of the end — the bug
        // this guards.
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
}
