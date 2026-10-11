// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Sidebar/main-area split layout: percentage-based split state, drag
//! clamping, and seam hit-testing for the two-panel console layout.
//!
//! Not responsible for: computing final pixel rects (see `layout`) or
//! rendering either panel.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragState {
    pub anchor_pct: u16,
    pub anchor_x: u16,
}

pub const MIN_SPLIT_PCT: u16 = 20;
pub const MAX_SPLIT_PCT: u16 = 80;
pub const DEFAULT_SPLIT_PCT: u16 = 30;

#[must_use]
pub const fn clamp_split(pct: u16) -> u16 {
    if pct < MIN_SPLIT_PCT {
        MIN_SPLIT_PCT
    } else if pct > MAX_SPLIT_PCT {
        MAX_SPLIT_PCT
    } else {
        pct
    }
}

/// Two-pane split rects carried by the upstream `ResizablePanelGroup` in
/// seamless mode (`handle_cells(0)`): the console renders adjacent panes with
/// its own seam affordance, so no handle column is reserved. The
/// percentage-to-cells math stays consumer-side (the ratatui `Percentage`
/// solver seeded via `set_sizes_cells`), keeping the geometry byte-identical
/// to the pre-adoption hand-rolled split. The seam-drag lane (hit slack,
/// anchor-relative delta, pct clamp, width gate) stays consumer code — see
/// `layout` and `input::mouse`.
#[must_use]
pub fn split_panel_group_layout(
    area: ratatui::layout::Rect,
    left_pct: u16,
) -> termrock::widgets::ResizablePanelGroupLayout {
    use ratatui::layout::{Constraint, Direction, Layout};
    use termrock::widgets::{
        PanelId, ResizablePanelGroup, ResizablePanelGroupState, ResizablePanelSpec,
    };

    let right_pct = 100u16.saturating_sub(left_pct);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(left_pct),
            Constraint::Percentage(right_pct),
        ])
        .split(area);
    let specs = [
        ResizablePanelSpec::main(PanelId::from_static("list"), left_pct.max(1)).min(0),
        ResizablePanelSpec::main(PanelId::from_static("preview"), right_pct.max(1)).min(0),
    ];
    let system = termrock::style::DesignSystem::default();
    let group = ResizablePanelGroup::new(&specs, &system).handle_cells(0);
    let mut state = ResizablePanelGroupState::new();
    state.set_sizes_cells(&[columns[0].width, columns[1].width]);
    group.layout(area, &mut state)
}
