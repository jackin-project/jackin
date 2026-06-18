//! Top-level console frame composition helpers.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::tui::app::ConsoleManagerStageRoute;
use crate::tui::app::ConsoleStageModalFacts;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleModalPrepareAreas {
    pub list_modal: Rect,
    pub stage_modal: Option<StageModalArea>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageFooterHeightFacts {
    pub route: ConsoleManagerStageRoute,
    pub workspace_footer_height: u16,
    pub editor_footer_height: u16,
    pub settings_footer_height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleMainFramePlan {
    Editor,
    Settings,
    Workspace { render_list_body: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolePrepareFramePlan {
    Editor,
    Settings,
    List,
    None,
}

#[must_use]
pub const fn console_main_frame_plan(route: ConsoleManagerStageRoute) -> ConsoleMainFramePlan {
    match route {
        ConsoleManagerStageRoute::Editor => ConsoleMainFramePlan::Editor,
        ConsoleManagerStageRoute::Settings => ConsoleMainFramePlan::Settings,
        ConsoleManagerStageRoute::List => ConsoleMainFramePlan::Workspace {
            render_list_body: true,
        },
        ConsoleManagerStageRoute::CreatePrelude
        | ConsoleManagerStageRoute::ConfirmDelete
        | ConsoleManagerStageRoute::ConfirmInstancePurge => ConsoleMainFramePlan::Workspace {
            render_list_body: false,
        },
    }
}

#[must_use]
pub const fn console_prepare_frame_plan(
    route: ConsoleManagerStageRoute,
) -> ConsolePrepareFramePlan {
    match route {
        ConsoleManagerStageRoute::Editor => ConsolePrepareFramePlan::Editor,
        ConsoleManagerStageRoute::Settings => ConsolePrepareFramePlan::Settings,
        ConsoleManagerStageRoute::List => ConsolePrepareFramePlan::List,
        ConsoleManagerStageRoute::CreatePrelude
        | ConsoleManagerStageRoute::ConfirmDelete
        | ConsoleManagerStageRoute::ConfirmInstancePurge => ConsolePrepareFramePlan::None,
    }
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
pub const fn modal_overlay_state_from_stage_facts(
    status_overlay: bool,
    list_modal: bool,
    stage: ConsoleStageModalFacts,
) -> ModalOverlayState {
    ModalOverlayState {
        status_overlay,
        list_modal,
        editor_modal: stage.editor_modal_open,
        settings_error: stage.settings_error_popup_open,
        settings_mounts_modal: stage.settings_mounts_modal_open,
        settings_env_modal: stage.settings_env_modal_open,
        settings_auth_modal: stage.settings_auth_modal_open,
        create_prelude_modal: stage.create_prelude_modal_open,
        destructive_confirm: stage.destructive_confirm_open,
    }
}

#[must_use]
pub const fn modal_overlay_state_for_route(
    route: ConsoleManagerStageRoute,
    status_overlay: bool,
    list_modal_open: bool,
    stage: ConsoleStageModalFacts,
) -> ModalOverlayState {
    modal_overlay_state_from_stage_facts(
        status_overlay,
        matches!(route, ConsoleManagerStageRoute::List) && list_modal_open,
        stage,
    )
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
pub const fn modal_backdrop_area(area: Rect, footer_height: u16) -> Rect {
    modal_content_area(area, footer_height)
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
pub const fn visible_modal_prepare_areas(
    area: Rect,
    workspace_footer_height: u16,
    editor_footer_height: u16,
    settings_footer_height: u16,
    route: ConsoleManagerStageRoute,
) -> VisibleModalPrepareAreas {
    let areas = modal_content_areas(
        area,
        workspace_footer_height,
        editor_footer_height,
        settings_footer_height,
    );
    VisibleModalPrepareAreas {
        list_modal: areas.workspace,
        stage_modal: stage_modal_area_for_route(route, areas),
    }
}

#[must_use]
pub const fn visible_modal_prepare_areas_for_stage_facts(
    area: Rect,
    facts: StageFooterHeightFacts,
) -> VisibleModalPrepareAreas {
    visible_modal_prepare_areas(
        area,
        facts.workspace_footer_height,
        if matches!(facts.route, ConsoleManagerStageRoute::Editor) {
            facts.editor_footer_height
        } else {
            0
        },
        if matches!(facts.route, ConsoleManagerStageRoute::Settings) {
            facts.settings_footer_height
        } else {
            0
        },
        facts.route,
    )
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
