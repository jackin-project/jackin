// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Frame composition and overlay-state plans.

use super::{
    ConsoleMainFramePlan, ConsoleModalRenderPlan, ConsolePrepareFramePlan,
    ConsoleReservedFooterHeightPlan, ModalOverlayState, ReservedFooterHeightFacts,
};

use crate::tui::model::ConsoleManagerStageRoute;
use crate::tui::model::ConsoleStageModalFacts;

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
pub const fn console_modal_render_plan(route: ConsoleManagerStageRoute) -> ConsoleModalRenderPlan {
    match route {
        ConsoleManagerStageRoute::List => ConsoleModalRenderPlan::List,
        ConsoleManagerStageRoute::Editor => ConsoleModalRenderPlan::Editor,
        ConsoleManagerStageRoute::Settings => ConsoleModalRenderPlan::Settings,
        ConsoleManagerStageRoute::CreatePrelude => ConsoleModalRenderPlan::CreatePrelude,
        ConsoleManagerStageRoute::ConfirmDelete => ConsoleModalRenderPlan::ConfirmDelete,
        ConsoleManagerStageRoute::ConfirmInstancePurge => {
            ConsoleModalRenderPlan::ConfirmInstancePurge
        }
    }
}

#[must_use]
pub const fn console_reserved_footer_height_plan(
    route: ConsoleManagerStageRoute,
) -> ConsoleReservedFooterHeightPlan {
    match route {
        ConsoleManagerStageRoute::Editor => ConsoleReservedFooterHeightPlan::Editor,
        ConsoleManagerStageRoute::Settings => ConsoleReservedFooterHeightPlan::Settings,
        ConsoleManagerStageRoute::List
        | ConsoleManagerStageRoute::CreatePrelude
        | ConsoleManagerStageRoute::ConfirmDelete
        | ConsoleManagerStageRoute::ConfirmInstancePurge => {
            ConsoleReservedFooterHeightPlan::Workspace
        }
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
    !matches!(state, ModalOverlayState::None)
}

#[must_use]
pub const fn modal_overlay_state_from_stage_facts(
    status_overlay: bool,
    list_modal: bool,
    stage: ConsoleStageModalFacts,
) -> ModalOverlayState {
    if status_overlay {
        ModalOverlayState::Status
    } else if list_modal {
        ModalOverlayState::List
    } else if stage.editor_modal_open {
        ModalOverlayState::Editor
    } else if stage.settings_error_popup_open {
        ModalOverlayState::SettingsError
    } else if stage.settings_mounts_modal_open {
        ModalOverlayState::SettingsMounts
    } else if stage.settings_env_modal_open {
        ModalOverlayState::SettingsEnv
    } else if stage.settings_auth_modal_open {
        ModalOverlayState::SettingsAuth
    } else if stage.create_prelude_modal_open {
        ModalOverlayState::CreatePrelude
    } else if stage.destructive_confirm_open {
        ModalOverlayState::DestructiveConfirm
    } else {
        ModalOverlayState::None
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
