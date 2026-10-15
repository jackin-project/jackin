// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::model::ConsoleManagerStageRoute;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFrameAreas {
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModalOverlayState {
    #[default]
    None,
    Status,
    List,
    Editor,
    SettingsError,
    SettingsMounts,
    SettingsEnv,
    SettingsAuth,
    CreatePrelude,
    DestructiveConfirm,
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
    Workflows,
    Workspace { render_list_body: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolePrepareFramePlan {
    Editor,
    Settings,
    List,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleModalRenderPlan {
    List,
    Editor,
    Settings,
    Workflows,
    CreatePrelude,
    ConfirmDelete,
    ConfirmInstancePurge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleReservedFooterHeightPlan {
    Workspace,
    Editor,
    Settings,
}
