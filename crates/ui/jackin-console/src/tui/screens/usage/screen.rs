// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Usage screen key handling and entry render.

use super::{now_epoch, render_account_list, render_detail};

use crossterm::event::{KeyCode, KeyEvent};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

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

/// Render one usage frame against one sampled wall-clock epoch.
///
/// The production entry point samples once and delegates here. Keeping the
/// epoch explicit at this seam lets deterministic render tests use the same
/// frame clock while ensuring the list and detail panes cannot cross a
/// relative-time boundary independently.
pub fn render_at(frame: &mut Frame<'_>, area: Rect, state: &ManagerState<'_>, now_epoch: i64) {
    let body = crate::tui::view::workspace_frame_areas(area).body;
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(body);
    render_account_list(frame, columns[0], state, now_epoch);
    render_detail(frame, columns[1], state, now_epoch);
}
