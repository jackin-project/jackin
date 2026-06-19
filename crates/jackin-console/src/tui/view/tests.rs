//! Tests for `view`.
mod consistency;

use super::*;
use crate::tui::app::{ConsoleManagerStageRoute, ConsoleStageModalFacts};

#[test]
fn console_main_frame_plan_routes_workspace_and_fullscreen_stages() {
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::Editor),
        ConsoleMainFramePlan::Editor
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::Settings),
        ConsoleMainFramePlan::Settings
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::List),
        ConsoleMainFramePlan::Workspace {
            render_list_body: true
        }
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleMainFramePlan::Workspace {
            render_list_body: false
        }
    );
    assert_eq!(
        console_main_frame_plan(ConsoleManagerStageRoute::ConfirmInstancePurge),
        ConsoleMainFramePlan::Workspace {
            render_list_body: false
        }
    );
}

#[test]
fn console_prepare_frame_plan_routes_only_mutating_pre_render_stages() {
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::Editor),
        ConsolePrepareFramePlan::Editor
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::Settings),
        ConsolePrepareFramePlan::Settings
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::List),
        ConsolePrepareFramePlan::List
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsolePrepareFramePlan::None
    );
    assert_eq!(
        console_prepare_frame_plan(ConsoleManagerStageRoute::ConfirmDelete),
        ConsolePrepareFramePlan::None
    );
}

#[test]
fn console_modal_render_plan_routes_modal_families() {
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::List),
        ConsoleModalRenderPlan::List
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::Editor),
        ConsoleModalRenderPlan::Editor
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::Settings),
        ConsoleModalRenderPlan::Settings
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleModalRenderPlan::CreatePrelude
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::ConfirmDelete),
        ConsoleModalRenderPlan::ConfirmDelete
    );
    assert_eq!(
        console_modal_render_plan(ConsoleManagerStageRoute::ConfirmInstancePurge),
        ConsoleModalRenderPlan::ConfirmInstancePurge
    );
}

#[test]
fn console_reserved_footer_height_plan_routes_screen_footers() {
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::Editor),
        ConsoleReservedFooterHeightPlan::Editor
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::Settings),
        ConsoleReservedFooterHeightPlan::Settings
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::List),
        ConsoleReservedFooterHeightPlan::Workspace
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleReservedFooterHeightPlan::Workspace
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::ConfirmDelete),
        ConsoleReservedFooterHeightPlan::Workspace
    );
    assert_eq!(
        console_reserved_footer_height_plan(ConsoleManagerStageRoute::ConfirmInstancePurge),
        ConsoleReservedFooterHeightPlan::Workspace
    );
}

#[test]
fn workspace_frame_areas_match_header_body_footer_contract() {
    let areas = workspace_frame_areas(Rect::new(0, 0, 80, 24));

    assert_eq!(areas.header, Rect::new(0, 0, 80, 2));
    assert_eq!(areas.body, Rect::new(0, 2, 80, 20));
    assert_eq!(areas.footer, Rect::new(0, 22, 80, 2));
}

#[test]
fn modal_content_area_reserves_footer_height() {
    let area = Rect::new(3, 4, 80, 24);

    assert_eq!(modal_content_area(area, 3), Rect::new(3, 4, 80, 21));
}

#[test]
fn modal_backdrop_area_reserves_footer_height() {
    let area = Rect::new(3, 4, 80, 24);

    assert_eq!(modal_backdrop_area(area, 3), Rect::new(3, 4, 80, 21));
}

#[test]
fn modal_content_area_saturates_when_footer_exceeds_height() {
    let area = Rect::new(3, 4, 80, 2);

    assert_eq!(modal_content_area(area, 3), Rect::new(3, 4, 80, 0));
}

#[test]
fn modal_content_areas_reserve_screen_specific_footers() {
    let area = Rect::new(3, 4, 80, 24);

    assert_eq!(
        modal_content_areas(area, 2, 4, 6),
        ModalContentAreas {
            workspace: Rect::new(3, 4, 80, 22),
            editor: Rect::new(3, 4, 80, 20),
            settings: Rect::new(3, 4, 80, 18),
        }
    );
}

#[test]
fn stage_modal_area_routes_by_visible_stage() {
    let areas = ModalContentAreas {
        workspace: Rect::new(0, 0, 10, 20),
        editor: Rect::new(1, 0, 10, 18),
        settings: Rect::new(2, 0, 10, 16),
    };

    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::Editor, areas),
        Some(StageModalArea::Editor(areas.editor))
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::Settings, areas),
        Some(StageModalArea::Settings(areas.settings))
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::CreatePrelude, areas),
        Some(StageModalArea::Workspace(areas.workspace))
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::List, areas),
        None
    );
    assert_eq!(
        stage_modal_area_for_route(ConsoleManagerStageRoute::ConfirmDelete, areas),
        None
    );
}

#[test]
fn visible_modal_prepare_areas_routes_list_and_stage_modals() {
    let plan = visible_modal_prepare_areas(
        Rect::new(0, 0, 80, 24),
        2,
        4,
        6,
        ConsoleManagerStageRoute::Settings,
    );

    assert_eq!(plan.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(
        plan.stage_modal,
        Some(StageModalArea::Settings(Rect::new(0, 0, 80, 18)))
    );

    let list_plan = visible_modal_prepare_areas(
        Rect::new(0, 0, 80, 24),
        2,
        4,
        6,
        ConsoleManagerStageRoute::List,
    );
    assert_eq!(list_plan.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(list_plan.stage_modal, None);
}

#[test]
fn visible_modal_prepare_areas_for_stage_facts_uses_active_stage_footer() {
    let area = Rect::new(0, 0, 80, 24);

    let editor = visible_modal_prepare_areas_for_stage_facts(
        area,
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Editor,
            workspace_footer_height: 2,
            editor_footer_height: 4,
            settings_footer_height: 6,
        },
    );
    assert_eq!(editor.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(
        editor.stage_modal,
        Some(StageModalArea::Editor(Rect::new(0, 0, 80, 20)))
    );

    let settings = visible_modal_prepare_areas_for_stage_facts(
        area,
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Settings,
            workspace_footer_height: 2,
            editor_footer_height: 4,
            settings_footer_height: 6,
        },
    );
    assert_eq!(
        settings.stage_modal,
        Some(StageModalArea::Settings(Rect::new(0, 0, 80, 18)))
    );

    let list = visible_modal_prepare_areas_for_stage_facts(
        area,
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::List,
            workspace_footer_height: 2,
            editor_footer_height: 4,
            settings_footer_height: 6,
        },
    );
    assert_eq!(list.list_modal, Rect::new(0, 0, 80, 22));
    assert_eq!(list.stage_modal, None);
}

#[test]
fn reserved_footer_height_prefers_screen_specific_heights() {
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: Some(4),
            settings_footer_height: Some(6),
            workspace_footer_height: 2,
        }),
        4
    );
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: None,
            settings_footer_height: Some(6),
            workspace_footer_height: 2,
        }),
        6
    );
    assert_eq!(
        reserved_footer_height_for_facts(ReservedFooterHeightFacts {
            editor_footer_height: None,
            settings_footer_height: None,
            workspace_footer_height: 2,
        }),
        2
    );
}

#[test]
fn footer_height_helpers_keep_one_row_minimum() {
    assert_eq!(effective_footer_height(0), 1);
    assert_eq!(effective_footer_height(3), 3);
    assert_eq!(measured_footer_height(&[], 80), footer_height(&[], 80));
    assert!(measured_footer_height(&[], 80) >= 1);
}

#[test]
fn workspace_header_title_is_view_owned() {
    assert_eq!(workspace_header_title(), "workspaces");
}

#[test]
fn modal_areas_stable_preferred_size() {
    // On a wide terminal (300 cols) each dialog holds its preferred width
    // (pct_w% of the 160-col reference), not a fraction of the terminal.
    let wide = Rect::new(0, 0, 300, 40);
    assert_eq!(delete_confirm_area(wide).width, 96); // 60% of 160 = 96
    assert_eq!(delete_confirm_area(wide).height, 7);
    assert_eq!(purge_confirm_area(wide).width, 112); // 70% of 160 = 112
    assert_eq!(purge_confirm_area(wide).height, 9);
    assert_eq!(status_overlay_area(wide).width, 80); // 50% of 160 = 80
    assert_eq!(status_overlay_area(wide).height, 7);

    // On a narrow terminal (50 cols), dialogs shrink to terminal_width - 4 margin.
    let narrow = Rect::new(0, 0, 50, 40);
    assert_eq!(delete_confirm_area(narrow).width, 46); // min(96, 50-4) = 46
    assert_eq!(status_overlay_area(narrow).width, 46); // min(80, 46) = 46
}

#[test]
fn modal_overlay_visible_tracks_any_modal_fact() {
    assert!(!modal_overlay_visible(ModalOverlayState::default()));
    assert!(modal_overlay_visible(ModalOverlayState {
        status_overlay: true,
        ..ModalOverlayState::default()
    }));
    assert!(modal_overlay_visible(ModalOverlayState {
        settings_auth_modal: true,
        ..ModalOverlayState::default()
    }));
    assert!(modal_overlay_visible(ModalOverlayState {
        destructive_confirm: true,
        ..ModalOverlayState::default()
    }));
}

#[test]
fn modal_overlay_state_maps_stage_facts_and_outer_flags() {
    let overlay = modal_overlay_state_from_stage_facts(
        true,
        true,
        ConsoleStageModalFacts {
            editor_modal_open: true,
            settings_error_popup_open: true,
            settings_auth_modal_open: true,
            destructive_confirm_open: true,
            ..ConsoleStageModalFacts::default()
        },
    );

    assert_eq!(
        overlay,
        ModalOverlayState {
            status_overlay: true,
            list_modal: true,
            editor_modal: true,
            settings_error: true,
            settings_mounts_modal: false,
            settings_env_modal: false,
            settings_auth_modal: true,
            create_prelude_modal: false,
            destructive_confirm: true,
        }
    );
    assert!(modal_overlay_visible(overlay));
}

#[test]
fn modal_overlay_state_counts_list_modal_only_on_list_route() {
    let list = modal_overlay_state_for_route(
        ConsoleManagerStageRoute::List,
        false,
        true,
        ConsoleStageModalFacts::default(),
    );
    let editor = modal_overlay_state_for_route(
        ConsoleManagerStageRoute::Editor,
        false,
        true,
        ConsoleStageModalFacts::default(),
    );

    assert!(list.list_modal);
    assert!(!editor.list_modal);
}
