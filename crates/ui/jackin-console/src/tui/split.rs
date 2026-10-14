// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
