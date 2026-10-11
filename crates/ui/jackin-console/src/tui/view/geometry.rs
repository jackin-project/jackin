// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Modal and header area geometry.

use super::{
    ModalContentAreas, StageFooterHeightFacts, StageModalArea, VisibleModalPrepareAreas,
    WorkspaceFrameAreas,
};
use crate::tui::model::ConsoleManagerStageRoute;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

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
