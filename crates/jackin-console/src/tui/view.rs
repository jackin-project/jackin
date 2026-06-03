//! Top-level console frame composition helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFrameAreas {
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModalOverlayState {
    pub status_overlay: bool,
    pub list_modal: bool,
    pub editor_modal: bool,
    pub settings_error: bool,
    pub settings_mounts_modal: bool,
    pub settings_env_modal: bool,
    pub settings_auth_modal: bool,
    pub create_prelude_modal: bool,
    pub destructive_confirm: bool,
}

#[must_use]
pub const fn modal_overlay_visible(state: ModalOverlayState) -> bool {
    state.status_overlay
        || state.list_modal
        || state.editor_modal
        || state.settings_error
        || state.settings_mounts_modal
        || state.settings_env_modal
        || state.settings_auth_modal
        || state.create_prelude_modal
        || state.destructive_confirm
}

#[must_use]
pub fn workspace_frame_areas(area: Rect) -> WorkspaceFrameAreas {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(10),
            Constraint::Length(2),
        ])
        .split(area);
    WorkspaceFrameAreas {
        header: chunks[0],
        body: chunks[1],
        footer: chunks[2],
    }
}

#[must_use]
pub const fn workspace_header_title() -> &'static str {
    "workspaces"
}

/// How many rows the footer needs to display all `items` within `width`
/// columns. Minimum 1.
#[must_use]
pub fn footer_height(items: &[jackin_tui::HintSpan<'_>], width: u16) -> u16 {
    jackin_tui::components::wrapped_height(items, width)
}

pub fn render_footer(frame: &mut Frame, area: Rect, items: &[jackin_tui::HintSpan<'_>]) {
    jackin_tui::components::render_wrapped_hint_bar(frame, area, items);
}

pub fn render_header(frame: &mut Frame, area: Rect, title: &str) {
    jackin_tui::components::render_brand_header(frame, area, title);
}

pub fn render_modal_backdrop(frame: &mut Frame, area: Rect) {
    frame.render_widget(jackin_tui::components::ModalBackdrop, area);
}

#[must_use]
pub fn delete_confirm_area(area: Rect) -> Rect {
    crate::tui::layout::centered_rect_fixed(area, 60, 7)
}

#[must_use]
pub fn purge_confirm_area(area: Rect) -> Rect {
    crate::tui::layout::centered_rect_fixed(area, 70, 9)
}

#[must_use]
pub fn settings_error_area(area: Rect, height: u16) -> Rect {
    crate::tui::layout::centered_rect_fixed(area, 60, height)
}

#[must_use]
pub fn status_overlay_area(area: Rect) -> Rect {
    crate::tui::layout::centered_rect_fixed(area, 50, 7)
}

#[cfg(test)]
mod tests;
