// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Console frame area types and plans.

use crate::tui::model::ConsoleManagerStageRoute;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceFrameAreas {
    pub header: Rect,
    pub body: Rect,
    pub footer: Rect,
}

/// Which modal (if any) currently owns the overlay chrome.
///
/// R6 refactored into an enum from a 9-bool `ModalOverlayState` struct: in practice the
/// console presents exactly one modal at a time, so the OR'd bool set was
/// always reduced to "which one is on top" by the render layer. The enum
/// makes that priority order explicit instead of relying on field order in
/// a struct literal. Priority (highest first): `Status`, `List`, `Editor`,
/// `SettingsError`, `SettingsMounts`, `SettingsEnv`, `SettingsAuth`,
/// `CreatePrelude`, `DestructiveConfirm`.
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
