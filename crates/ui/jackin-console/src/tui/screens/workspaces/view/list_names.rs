// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Workspace list-names rendering.

use super::{
    WorkspaceListDisplayRow, WorkspaceListNamesRenderFacts, WorkspaceListNamesRenderPlan,
    WorkspaceListRowTone, panel, push_tree_instance_line, push_tree_workspace_line,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};

pub fn list_name_lines(
    visual_rows: &[Option<WorkspaceListDisplayRow>],
    viewport: usize,
    show_cursor: bool,
) -> (Vec<Line<'static>>, usize) {
    // Structural exception: workspace names are a mixed tree with disclosure,
    // instance tones, hover fill, and horizontal scroll padding, so they cannot
    // use the flat picker renderer even though they share its cursor contract.
    let mut max_w = viewport;
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(visual_rows.len());

    for visual_row in visual_rows {
        let Some(row) = visual_row else {
            lines.push(Line::from(""));
            continue;
        };
        match row.tone {
            WorkspaceListRowTone::Instance => {
                push_tree_instance_line(&mut lines, row, show_cursor, &mut max_w);
            }
            WorkspaceListRowTone::White | WorkspaceListRowTone::Workspace => {
                push_tree_workspace_line(&mut lines, row, show_cursor, &mut max_w);
            }
        }
    }

    let content_w = termrock::scroll::max_line_width(&lines).max(max_w);

    if let Some(selected_idx) = visual_rows
        .iter()
        .position(|row| row.as_ref().is_some_and(|row| row.selected))
        && let Some(line) = lines.get_mut(selected_idx)
    {
        let current_w = line.width();
        if current_w < content_w {
            let bg = match visual_rows[selected_idx].as_ref().map(|row| row.tone) {
                Some(WorkspaceListRowTone::Instance) => jackin_tui::tokens::CYAN,
                _ => termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::Accent)
                    .fg
                    .unwrap_or_default(),
            };
            line.spans.push(Span::styled(
                " ".repeat(content_w - current_w),
                Style::default().bg(bg).fg(Color::Black),
            ));
        }
    }

    if let Some(hovered_idx) = visual_rows
        .iter()
        .position(|row| row.as_ref().is_some_and(|row| row.hovered && !row.selected))
        && let Some(line) = lines.get_mut(hovered_idx)
    {
        for span in &mut line.spans {
            span.style = span.style.bg(termrock::style::DesignSystem::default()
                .style(termrock::style::Role::TabInactiveHovered)
                .bg
                .unwrap_or_default());
        }
        let current_w = line.width();
        if current_w < content_w {
            line.spans.push(Span::styled(
                " ".repeat(content_w - current_w),
                Style::default().bg(termrock::style::DesignSystem::default()
                    .style(termrock::style::Role::TabInactiveHovered)
                    .bg
                    .unwrap_or_default()),
            ));
        }
    }

    (lines, content_w)
}

#[must_use]
pub fn workspace_list_names_render_plan(
    facts: WorkspaceListNamesRenderFacts,
) -> WorkspaceListNamesRenderPlan {
    let viewport_h = usize::from(facts.area.height.saturating_sub(2));
    WorkspaceListNamesRenderPlan {
        viewport_width: termrock::scroll::viewport_width(facts.area),
        follow_scroll_y: names_window_follow_y(
            facts.selected_index,
            facts.row_count,
            viewport_h,
            facts.scroll_y,
        ),
    }
}

/// Names-list vertical window, driven by upstream `VirtualListState`
/// (plan 009): the stored offset seeds the virtualizer (`set_offset`, same
/// clamp) and `reveal` moves it only when the cursor leaves the view —
/// byte-identical to the retired `cursor_follow_offset` call. Zero height
/// short-circuits first: upstream `cursor_follow_offset` returns 0 there
/// while `VirtualListState` floors the viewport at 1, which would diverge.
/// One-shot construction per frame: the list keeps no persistent
/// virtual-list state of its own (the stored offset lives in
/// `ManagerState::list_names_scroll`, read by paint through this plan).
pub(crate) fn names_window_follow_y(
    selected_index: usize,
    row_count: usize,
    viewport_h: usize,
    scroll_y: u16,
) -> u16 {
    if viewport_h == 0 {
        return 0;
    }
    let mut window = termrock::widgets::VirtualListState::<usize>::new();
    window.set_logical_len(u64::try_from(row_count).unwrap_or(u64::MAX));
    window.set_viewport_extent(u16::try_from(viewport_h).unwrap_or(u16::MAX));
    window.set_offset(u64::from(scroll_y));
    let _ = window.reveal(u64::try_from(selected_index).unwrap_or(u64::MAX));
    u16::try_from(window.offset()).unwrap_or(u16::MAX)
}

pub fn render_list_names_block(
    frame: &mut Frame<'_>,
    area: Rect,
    lines: Vec<Line<'static>>,
    content_width: usize,
    focused: bool,
    scroll_x: u16,
    scroll_y: u16,
) {
    let content_height = lines.len();
    let viewport_w = termrock::scroll::viewport_width(area);
    let viewport_h = termrock::scroll::viewport_height(area);
    let h_scrollable = termrock::scroll::is_scrollable(content_width, viewport_w);
    let v_scrollable = termrock::scroll::is_scrollable(content_height, viewport_h);
    let theme = termrock::style::DesignSystem::default();
    let block = panel(&theme, None, focused).block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let visible_rows = usize::from(inner.height).min(content_height);
    let offset_y = usize::from(scroll_y).min(content_height.saturating_sub(visible_rows));
    for (row_idx, line) in lines
        .into_iter()
        .skip(offset_y)
        .take(visible_rows)
        .enumerate()
    {
        render_list_name_line(frame, inner, row_idx as u16, line, usize::from(scroll_x));
    }
    if h_scrollable {
        termrock::scroll::render_scrollbar(
            frame.buffer_mut(),
            termrock::scroll::horizontal_scrollbar_area(area),
            termrock::scroll::ScrollbarSpec::new(
                termrock::scroll::ScrollAxis::Horizontal,
                termrock::scroll::ScrollbarGeometry::new(content_width, viewport_w, scroll_x),
            ),
            &theme,
        );
    }
    if v_scrollable {
        termrock::scroll::render_scrollbar(
            frame.buffer_mut(),
            termrock::scroll::vertical_scrollbar_area(area),
            termrock::scroll::ScrollbarSpec::new(
                termrock::scroll::ScrollAxis::Vertical,
                termrock::scroll::ScrollbarGeometry::new(content_height, viewport_h, scroll_y),
            ),
            &theme,
        );
    }
}

pub(crate) fn render_list_name_line(
    frame: &mut Frame<'_>,
    area: Rect,
    row: u16,
    line: Line<'static>,
    scroll_x: usize,
) {
    pub(crate) const PREFIX_COLS: usize = 3;
    termrock::scroll::render_line_with_fixed_prefix_scroll(
        frame,
        area,
        row,
        line,
        PREFIX_COLS,
        scroll_x,
    );
}

pub(crate) fn row_fg(row: &WorkspaceListDisplayRow) -> Color {
    match row.tone {
        WorkspaceListRowTone::White => termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Text)
            .fg
            .unwrap_or_default(),
        WorkspaceListRowTone::Workspace => termrock::style::DesignSystem::default()
            .style(termrock::style::Role::Accent)
            .fg
            .unwrap_or_default(),
        WorkspaceListRowTone::Instance => jackin_tui::tokens::CYAN,
    }
}
