//! Top-level console frame composition helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::tui::app::ConsoleManagerStageRoute;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedFooterHeightFacts {
    pub editor_footer_height: Option<u16>,
    pub settings_footer_height: Option<u16>,
    pub workspace_footer_height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModalContentAreas {
    pub workspace: Rect,
    pub editor: Rect,
    pub settings: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageModalArea {
    Workspace(Rect),
    Editor(Rect),
    Settings(Rect),
}

#[must_use]
pub const fn reserved_footer_height_for_facts(facts: ReservedFooterHeightFacts) -> u16 {
    if let Some(height) = facts.editor_footer_height {
        return height;
    }
    if let Some(height) = facts.settings_footer_height {
        return height;
    }
    facts.workspace_footer_height
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

/// Full terminal area with bottom footer rows reserved for hints/status.
#[must_use]
pub const fn modal_content_area(area: Rect, footer_height: u16) -> Rect {
    Rect {
        height: area.height.saturating_sub(footer_height),
        ..area
    }
}

#[must_use]
pub const fn modal_content_areas(
    area: Rect,
    workspace_footer_height: u16,
    editor_footer_height: u16,
    settings_footer_height: u16,
) -> ModalContentAreas {
    ModalContentAreas {
        workspace: modal_content_area(area, workspace_footer_height),
        editor: modal_content_area(area, editor_footer_height),
        settings: modal_content_area(area, settings_footer_height),
    }
}

#[must_use]
pub const fn stage_modal_area_for_route(
    route: ConsoleManagerStageRoute,
    areas: ModalContentAreas,
) -> Option<StageModalArea> {
    match route {
        ConsoleManagerStageRoute::List
        | ConsoleManagerStageRoute::ConfirmDelete
        | ConsoleManagerStageRoute::ConfirmInstancePurge => None,
        ConsoleManagerStageRoute::Editor => Some(StageModalArea::Editor(areas.editor)),
        ConsoleManagerStageRoute::Settings => Some(StageModalArea::Settings(areas.settings)),
        ConsoleManagerStageRoute::CreatePrelude => Some(StageModalArea::Workspace(areas.workspace)),
    }
}

#[must_use]
pub const fn workspace_header_title() -> &'static str {
    "workspaces"
}

/// How many rows the footer needs to display all `items` within `width`
/// columns. Includes one leading blank spacer row above the hints.
#[must_use]
pub fn footer_height(items: &[jackin_tui::HintSpan<'_>], width: u16) -> u16 {
    // +1 for the mandatory leading spacer row above the hints on every screen.
    jackin_tui::components::wrapped_height(items, width).saturating_add(1)
}

#[must_use]
pub const fn effective_footer_height(height: u16) -> u16 {
    if height == 0 { 1 } else { height }
}

#[must_use]
pub fn measured_footer_height(items: &[jackin_tui::HintSpan<'_>], width: u16) -> u16 {
    effective_footer_height(footer_height(items, width))
}

pub fn render_footer(frame: &mut Frame<'_>, area: Rect, items: &[jackin_tui::HintSpan<'_>]) {
    if area.height == 0 {
        return;
    }
    // Render hints in the bottom portion; the top row is the leading spacer.
    let hint_rows = area.height.saturating_sub(1).max(1);
    let hint_area = Rect {
        x: area.x,
        y: area.y.saturating_add(area.height.saturating_sub(hint_rows)),
        width: area.width,
        height: hint_rows,
    };
    jackin_tui::components::render_wrapped_hint_bar(frame, hint_area, items);
}

pub fn render_header(frame: &mut Frame<'_>, area: Rect, title: &str) {
    jackin_tui::components::render_brand_header(frame, area, title);
}

pub fn render_modal_backdrop(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(jackin_tui::components::ModalBackdrop, area);
}

#[must_use]
pub fn delete_confirm_area(area: Rect) -> Rect {
    // Structural exception: legacy console confirm helpers wrap shared centering until all view modals are routed through `modal_rects`.
    crate::tui::layout::centered_rect_fixed(area, 60, 7)
}

#[must_use]
pub fn purge_confirm_area(area: Rect) -> Rect {
    // Structural exception: legacy console confirm helpers wrap shared centering until all view modals are routed through `modal_rects`.
    crate::tui::layout::centered_rect_fixed(area, 70, 9)
}

#[must_use]
pub fn settings_error_area(area: Rect, height: u16) -> Rect {
    // Structural exception: legacy console status/error helpers wrap shared centering while callers supply footer-excluded areas.
    crate::tui::layout::centered_rect_fixed(area, 60, height)
}

#[must_use]
pub fn status_overlay_area(area: Rect) -> Rect {
    crate::tui::layout::centered_rect_fixed(area, 50, 7)
}

#[cfg(test)]
mod tests;
