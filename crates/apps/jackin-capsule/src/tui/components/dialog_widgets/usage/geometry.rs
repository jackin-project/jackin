// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage-dialog geometry: areas, tab strip, title, and scroll inputs.

use super::{usage_info_lines, usage_info_lines_for_width, usage_line_width, usage_row_value};
use ratatui::layout::Rect;

pub(crate) fn usage_dialog_inner_area(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

pub(crate) fn usage_tab_strip_area(inner: Rect, tabs: &[(String, bool)]) -> Rect {
    let strip_width = usage_tab_strip_width(tabs)
        .saturating_sub(usize::from(termrock::widgets::TAB_GAP))
        .min(usize::from(inner.width));
    let strip_offset = usize::from(inner.width).saturating_sub(strip_width) / 2;
    Rect {
        x: inner
            .x
            .saturating_add(u16::try_from(strip_offset).unwrap_or(u16::MAX)),
        y: inner.y,
        width: u16::try_from(strip_width)
            .unwrap_or(inner.width)
            .max(1)
            .min(inner.width),
        height: inner.height.min(2),
    }
}

pub(crate) fn usage_tab_strip_index_at(
    tabs: &[(String, bool)],
    tab_area: Rect,
    col: u16,
) -> Option<usize> {
    let tab_refs = tabs
        .iter()
        .map(|(label, active)| (label.as_str(), *active))
        .collect::<Vec<_>>();
    termrock::widgets::lay_out_tabs(&tab_refs, tab_area.x)
        .iter()
        .position(|cell| {
            col >= cell.start_col
                && col < cell.start_col.saturating_add(cell.cell_cols)
                && tab_area.height > 0
        })
}

pub(crate) fn usage_tab_strip_labels(
    view: &jackin_protocol::control::FocusedUsageView,
    selected: crate::tui::components::dialog::UsageDialogTab,
) -> Vec<(String, bool)> {
    let overview_active = selected == crate::tui::components::dialog::UsageDialogTab::Overview;
    let mut tabs = vec![("Overview".to_owned(), overview_active)];
    tabs.extend(view.tabs.iter().map(|tab| {
        (
            usage_provider_display_label(&tab.label).to_owned(),
            !overview_active && tab.active,
        )
    }));
    tabs
}

pub(crate) fn usage_provider_display_label(label: &str) -> &str {
    // Lifted to jackin-usage so Desktop + Capsule share one remap (plan 008).
    jackin_usage::usage::provider_display_label(label)
}

pub(crate) fn usage_tab_strip_width(tabs: &[(String, bool)]) -> usize {
    let gap = usize::from(termrock::widgets::TAB_GAP);
    tabs.iter()
        .map(|(label, _)| termrock::text::display_cols(label) + 2 + gap)
        .sum()
}

/// Panel title. In the narrow list layout the provider-detail panel reads
/// `Usage: <provider>` from the Rust identity projection; the wide layout and
/// the Overview/Instance panels keep their own titles.
pub(crate) fn usage_panel_title(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    width: u16,
) -> String {
    let base = state.title();
    // Below 68 cols the full `Usage` title plus the provider detail no longer
    // fits the panel border, so collapse to the short `Usage: <provider>` form.
    // This trips a few cols before the body switches to the single-column list
    // layout (< 64) so the title is already compact when the rows reflow.
    if width >= 68 || base != "Usage" {
        return base.to_owned();
    }
    if let Some(provider) = usage_row_value(
        state,
        crate::tui::components::dialog::USAGE_IDENTITY_PROVIDER_ROW,
    ) {
        let short = provider.rsplit(" / ").next().unwrap_or(provider).trim();
        if !short.is_empty() {
            return format!("Usage: {short}");
        }
    }
    base.to_owned()
}

pub(crate) fn usage_info_required_height(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
) -> u16 {
    // Add the fixed chrome rows that frame the content (borders, title, and
    // padding) on top of the content-line count, then keep a 7-row floor so the
    // box stays usable when a provider has only a line or two to show.
    u16::try_from(usage_info_lines(state).len())
        .unwrap_or(u16::MAX)
        .saturating_add(5)
        .max(7)
}

/// The usage-dialog body rect (border **and** the 2-row tab strip removed).
/// Single source of truth for body geometry so the renderer and every
/// scroll-bound computation agree on the viewport (Bug 2). The tab strip is a
/// fixed 2 rows — `usage_tab_strip_area`'s height is `inner.height.min(2)`,
/// independent of tab count — so the body needs no tab list to compute.
pub(crate) fn usage_body_rect(box_rect: Rect) -> Rect {
    let inner = usage_dialog_inner_area(box_rect);
    let tab_h = inner.height.min(2);
    Rect {
        x: inner.x,
        y: inner.y.saturating_add(tab_h),
        width: inner.width,
        height: inner.height.saturating_sub(tab_h),
    }
}

/// Content size + the rect to feed the generic scroll helpers
/// (`dialog_scroll_axes` / `clamp_dialog_scroll`), derived from the **same**
/// width-wrapped line set the renderer uses, so the scroll bound can never
/// under- or over-shoot the rendered body (Bug 2).
///
/// Returns `(content_width, content_height, scroll_rect)` where `content_height`
/// is the wrapped line count at the body width, and `scroll_rect` is sized so
/// that `viewport_height(scroll_rect) == body.height` and
/// `viewport_width(scroll_rect) == body.width` (those helpers subtract the
/// 1-cell border; `scroll_rect.height = body.height + 2` re-adds exactly that so
/// the true body viewport — box minus border minus tab strip — is what clamps).
pub(crate) fn usage_scroll_inputs(
    box_rect: Rect,
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
) -> (usize, usize, Rect) {
    let body = usage_body_rect(box_rect);
    let lines = usage_info_lines_for_width(state, body.width);
    let content_width = lines.iter().map(usage_line_width).max().unwrap_or(0);
    let content_height = lines.len();
    let scroll_rect = Rect {
        height: body.height.saturating_add(2),
        ..box_rect
    };
    (content_width, content_height, scroll_rect)
}
