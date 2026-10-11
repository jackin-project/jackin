// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Pointer hover, chrome hit-testing, and drag-resize detection.

use ratatui::layout::Rect;
use termrock::interaction::HitRegion;

use crate::tui::components::branch_context_bar::{
    BranchContextBarHit, ColRange, branch_context_bar_layout, debug_run_id_label,
};

use crate::tui::terminal::osc22_pointer_shape;

use super::super::{
    ChromeHitState, DragState, HoverFramePlan, HoverState, HoverTarget, Multiplexer, PointerShape,
    PointerShapeState, SGR_NO_BUTTON_MOTION, STATUS_BAR_ROWS, chrome_hover_target_for_state,
    content_rect, drag_resize_ratio, drag_resize_redraw_reason, hover_frame_plan,
    hover_target_for_state, pointer_shape_for_state, status_change_redraw_reason,
};

use super::host_url_opening_allowed;

impl Multiplexer {
    pub(crate) fn set_pointer_shape(&mut self, shape: PointerShape) {
        if !self.client_registry.pointer_shapes_supported
            || self.client_registry.pointer_shape == shape
        {
            return;
        }
        self.client_registry.pointer_shape = shape;
        self.send_out_of_band(osc22_pointer_shape(shape));
    }

    pub(crate) fn update_pointer_shape_for_mouse(&mut self, row: u16, col: u16, button: u8) {
        if !self.client_registry.pointer_shapes_supported {
            return;
        }
        let shape = self.pointer_shape_at(row, col, button);
        self.set_pointer_shape(shape);
    }

    pub(crate) fn update_hover_for_mouse(&mut self, row: u16, col: u16, button: u8) {
        let next = self.hover_target_at(row, col);
        let next_link = self.link_hover_url_at(row, col, button);
        // The shared Debug info dialog brightens the hovered copyable row, so a
        // move between two copyable rows must redraw even though hover_target
        // stays DialogCopyTarget. Track the per-row hover separately.
        let (term_rows, term_cols) = self.render.terminal_size();
        let row_hover_changed = self.dialog_top_mut().is_some_and(|dialog| {
            let row = row + 1;
            let col = col + 1;
            dialog.set_container_info_hover(row, col, term_rows, term_cols)
                || dialog.set_usage_tab_hover(row, col, term_rows, term_cols)
        });
        if self.render.hover_target == next
            && self.render.link_hover_url == next_link
            && !row_hover_changed
        {
            return;
        }
        self.render.hover_target = next;
        self.render.link_hover_url = next_link;
        match hover_frame_plan(self.dialog_open()) {
            HoverFramePlan::DialogOverlay(reason) => self.invalidate(reason),
            HoverFramePlan::ChromeHover => self.invalidate(status_change_redraw_reason()),
        }
    }

    /// Resolve the chrome target a hit at `(row, col)` (0-based)
    /// would land on, walking dialog → tab strip → menu → branch bar
    /// in priority order. Both `hover_target_at` and `pointer_shape_at`
    /// consume this so the priority ordering lives once.
    pub(crate) fn chrome_hit_target_at(&self, row: u16, col: u16) -> Option<HoverTarget> {
        let row_1based = row + 1;
        let col_1based = col + 1;
        let dialog_copy_target = self.dialog_top().is_some_and(|dialog| {
            let github = self.github_context_view();
            dialog.clickable_at(
                row_1based,
                col_1based,
                self.render.term_rows,
                self.render.term_cols,
                Some(&github),
            )
        });
        if self.dialog_top().is_some() {
            return chrome_hover_target_for_state(ChromeHitState {
                dialog_copy_target,
                dialog_open: true,
                tab: None,
                menu_hit: false,
                branch_hit: None,
            });
        }

        let mut regions = Vec::new();
        self.register_chrome_hover_targets(&mut regions);
        let position = ratatui::layout::Position::new(col, row);
        let target = regions
            .iter()
            .find(|region| region.area.contains(position))
            .map(|region| region.id);
        chrome_hover_target_for_state(ChromeHitState {
            dialog_copy_target,
            dialog_open: false,
            tab: target.and_then(|target| match target {
                HoverTarget::Tab(idx) => Some(idx),
                _ => None,
            }),
            menu_hit: target == Some(HoverTarget::Menu),
            branch_hit: target.and_then(|target| match target {
                HoverTarget::BranchContext => Some(BranchContextBarHit::Context),
                HoverTarget::UsageStatus => Some(BranchContextBarHit::UsageStatus),
                HoverTarget::Container => Some(BranchContextBarHit::Container),
                HoverTarget::DebugChip => Some(BranchContextBarHit::DebugChip),
                _ => None,
            }),
        })
    }

    fn register_chrome_hover_targets(&self, regions: &mut Vec<HitRegion<HoverTarget>>) {
        for (idx, (start, end)) in self
            .status
            .status_bar
            .tab_regions
            .iter()
            .copied()
            .enumerate()
        {
            register_row0_range_1based(regions, start, end, HoverTarget::Tab(idx));
        }
        if let Some((start, end)) = self.status.status_bar.hint_region {
            register_row0_range_1based(regions, start, end, HoverTarget::Menu);
        }

        let Some(layout) = branch_context_bar_layout(
            self.render.term_rows,
            self.render.term_cols,
            self.context_bar_branch(),
            self.focused_usage_status_label().as_deref(),
            self.pr_watch.pull_request_context.as_deref(),
            self.pull_request_context_loading(),
            debug_run_id_label().as_deref(),
            self.status.status_bar.instance_id_label(),
        ) else {
            return;
        };
        let row0 = self.render.term_rows.saturating_sub(1);
        register_col_range_1based(regions, row0, layout.left, HoverTarget::BranchContext);
        register_col_range_1based(regions, row0, layout.usage, HoverTarget::UsageStatus);
        register_col_range_1based(regions, row0, layout.container, HoverTarget::Container);
        register_col_range_1based(regions, row0, layout.debug_chip, HoverTarget::DebugChip);
    }

    pub(crate) fn hover_target_at(&self, row: u16, col: u16) -> Option<HoverTarget> {
        hover_target_for_state(HoverState {
            dragging: self.render.drag.is_some(),
            selecting: self.clipboard.selection.is_some(),
            chrome_target: self.chrome_hit_target_at(row, col),
        })
    }

    pub(crate) fn pointer_shape_at(&self, row: u16, col: u16, button: u8) -> PointerShape {
        pointer_shape_for_state(PointerShapeState {
            dragging: self.render.drag.is_some(),
            selecting: self.clipboard.selection.is_some(),
            chrome_target: self.chrome_hit_target_at(row, col),
            dialog_open: self.dialog_top().is_some(),
            drag_start_orient: self.detect_drag_start(row, col).map(|drag| drag.orient),
            selection_start_available: self.detect_selection_start(row, col).is_some(),
            link_target_available: self.link_hover_url_at(row, col, button).is_some(),
            no_button_motion: button == SGR_NO_BUTTON_MOTION,
        })
    }

    fn link_hover_url_at(&self, row: u16, col: u16, button: u8) -> Option<String> {
        if self.dialog_top().is_some()
            || !host_url_opening_allowed()
            || !is_host_url_hover_button(button)
        {
            return None;
        }
        self.resolve_http_url_at_mouse_cell(row, col)
    }

    pub(crate) fn detect_drag_start(&self, row: u16, col: u16) -> Option<DragState> {
        if row < STATUS_BAR_ROWS || self.active_zoomed_id().is_some() {
            return None;
        }
        let content_rect = content_rect(self.render.content_rows, self.render.term_cols);
        let tab = self
            .session_supervisor
            .tabs
            .get(self.session_supervisor.active_tab)?;
        let (path, orient, rect) = tab.tree.border_at(content_rect, row, col)?;
        Some(DragState {
            tab_idx: self.session_supervisor.active_tab,
            path,
            orient,
            rect,
        })
    }

    pub(crate) fn drag_motion(&mut self, row: u16, col: u16) {
        let Some(drag) = self.render.drag.clone() else {
            return;
        };
        let new_ratio = drag_resize_ratio(drag.orient, drag.rect, row, col);
        let Some(tab) = self.session_supervisor.tabs.get_mut(drag.tab_idx) else {
            return;
        };
        if !tab.tree.set_ratio_at(&drag.path, new_ratio) {
            return;
        }
        self.resize_panes();
        self.invalidate(drag_resize_redraw_reason());
    }
}

fn is_host_url_hover_button(button: u8) -> bool {
    const ALT_MODIFIER: u8 = 8;
    const CTRL_MODIFIER: u8 = 16;
    const MOTION_MODIFIER: u8 = 32;

    let no_button_motion = button & 0b11 == 3;
    let modified = button & (ALT_MODIFIER | CTRL_MODIFIER) != 0;
    let motion = button & MOTION_MODIFIER != 0;
    no_button_motion && modified && motion
}

fn register_row0_range_1based(
    regions: &mut Vec<HitRegion<HoverTarget>>,
    start: u16,
    end: u16,
    key: HoverTarget,
) {
    if let Some(range) = ColRange::new(start, end) {
        register_col_range_1based(regions, 0, Some(range), key);
    }
}

fn register_col_range_1based(
    regions: &mut Vec<HitRegion<HoverTarget>>,
    row0: u16,
    range: Option<ColRange>,
    key: HoverTarget,
) {
    let Some(range) = range else {
        return;
    };
    let x = range.start.saturating_sub(1);
    let width = range.end.saturating_sub(range.start);
    if width > 0 {
        regions.push(HitRegion {
            id: key,
            area: Rect::new(x, row0, width, 1),
        });
    }
}
