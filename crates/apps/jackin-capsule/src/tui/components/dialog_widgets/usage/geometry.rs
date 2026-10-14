// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
    jackin_usage_provider_core::provider_display_label(label)
}

pub(crate) fn usage_tab_strip_width(tabs: &[(String, bool)]) -> usize {
    let gap = usize::from(termrock::widgets::TAB_GAP);
    tabs.iter()
        .map(|(label, _)| termrock::text::display_cols(label) + 2 + gap)
        .sum()
}

pub(crate) fn usage_panel_title(
    state: &crate::tui::components::container_info_surface::ContainerInfoState,
    width: u16,
) -> String {
    let base = state.title();

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
    u16::try_from(usage_info_lines(state).len())
        .unwrap_or(u16::MAX)
        .saturating_add(5)
        .max(7)
}

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
