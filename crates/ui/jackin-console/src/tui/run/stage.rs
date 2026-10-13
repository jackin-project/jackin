// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Console screen stage routing.

use crate::tui::model::ConsoleManagerStageRoute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleScreenStage {
    List,
    Editor,
    Settings,
    CreatePrelude,
    ConfirmDelete,
    ConfirmInstancePurge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleChromeHover {
    DebugChip,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MainScreenState {
    pub workspace_list: bool,
    pub list_modal_open: bool,
}

/// Bare `q` exits directly only from the plain workspace list. Other screens
/// or overlays use the quit confirmation flow.
#[must_use]
pub const fn is_main_screen(state: MainScreenState) -> bool {
    state.workspace_list && !state.list_modal_open
}

#[must_use]
pub const fn is_main_screen_for_route(
    route: ConsoleManagerStageRoute,
    list_modal_open: bool,
) -> bool {
    is_main_screen(MainScreenState {
        workspace_list: matches!(route, ConsoleManagerStageRoute::List),
        list_modal_open,
    })
}

#[must_use]
pub const fn console_screen_stage_for_route(route: ConsoleManagerStageRoute) -> ConsoleScreenStage {
    match route {
        ConsoleManagerStageRoute::List => ConsoleScreenStage::List,
        ConsoleManagerStageRoute::Editor => ConsoleScreenStage::Editor,
        ConsoleManagerStageRoute::Settings => ConsoleScreenStage::Settings,
        ConsoleManagerStageRoute::CreatePrelude => ConsoleScreenStage::CreatePrelude,
        ConsoleManagerStageRoute::ConfirmDelete => ConsoleScreenStage::ConfirmDelete,
        ConsoleManagerStageRoute::ConfirmInstancePurge => ConsoleScreenStage::ConfirmInstancePurge,
    }
}

/// Which diagnostics screen owns the visible console stage. Confirm dialogs
/// overlay the workspace list, so their telemetry remains attached to `List`.
#[must_use]
pub const fn diagnostics_screen_for_stage(
    stage: ConsoleScreenStage,
) -> jackin_telemetry::schema::enums::ScreenId {
    use jackin_telemetry::schema::enums::ScreenId;
    match stage {
        ConsoleScreenStage::List
        | ConsoleScreenStage::ConfirmDelete
        | ConsoleScreenStage::ConfirmInstancePurge => ScreenId::WorkspaceList,
        ConsoleScreenStage::Editor => ScreenId::WorkspaceEditor,
        ConsoleScreenStage::Settings => ScreenId::Settings,
        ConsoleScreenStage::CreatePrelude => ScreenId::WorkspaceCreate,
    }
}
