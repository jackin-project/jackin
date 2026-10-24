// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::list::render_account_list_with_geometry;
use super::{UsagePaintReceipt, now_epoch, render_detail, usage_frame_layout};

use crossterm::event::{KeyCode, KeyEvent};

use ratatui::{Frame, layout::Rect};

use crate::tui::state::ManagerState;

pub fn handle_key(state: &mut ManagerState<'_>, key: KeyEvent) {
    let Some(screen) = state.usage.screen.as_mut() else {
        return;
    };
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => state.usage.visible = false,
        KeyCode::Up | KeyCode::Char('k') => screen.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => screen.move_selection(1),
        KeyCode::Enter => screen.detail = !screen.detail,
        KeyCode::Char('s') => {
            screen.sort = screen.sort.cycle();
            screen.reanchor_after_view_change();
        }
        KeyCode::Char('f') => {
            screen.filter = screen.filter.cycle();
            screen.reanchor_after_view_change();
        }
        KeyCode::Char('c') => {
            screen.jump_to_most_constrained();
        }
        KeyCode::Char('r' | 'R') => {
            screen.refresh_due = true;
            screen.force_refresh_pending = true;
        }
        KeyCode::PageUp => screen.scroll = screen.scroll.saturating_sub(5),
        KeyCode::PageDown => {
            screen.scroll = screen.scroll.saturating_add(5);
        }
        _ => {}
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>) {
    render_at(frame, area, state, now_epoch());
}

pub fn render_at(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>, now_epoch: i64) {
    drop(render_at_with_pointer_receipt(
        frame, area, state, now_epoch,
    ));
}

pub(crate) fn render_at_with_pointer_receipt(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &ManagerState<'_>,
    now_epoch: i64,
) -> Option<UsagePaintReceipt> {
    let frame_area = frame.area();
    let request_is_fully_painted = area.width > 0
        && area.height > 0
        && u32::from(area.x) >= u32::from(frame_area.x)
        && u32::from(area.y) >= u32::from(frame_area.y)
        && u32::from(area.x) + u32::from(area.width)
            <= u32::from(frame_area.x) + u32::from(frame_area.width)
        && u32::from(area.y) + u32::from(area.height)
            <= u32::from(frame_area.y) + u32::from(frame_area.height);
    let layout = usage_frame_layout(area);
    let geometry = render_account_list_with_geometry(frame, layout.list_outer, state, now_epoch)?;
    render_detail(frame, layout.detail_outer, state, now_epoch);
    let screen = state.usage.screen.as_ref()?;
    (state.usage.visible && request_is_fully_painted)
        .then(|| UsagePaintReceipt::new(area, now_epoch, screen, layout, geometry))
}
