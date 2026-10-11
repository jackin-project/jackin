// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn term_width_scroll_plan_updates_and_clamps_offset() {
    assert_eq!(term_width_scroll_plan(0, 8, 10, 40), 8);
    assert_eq!(term_width_scroll_plan(8, -99, 10, 40), 0);
}

#[test]
fn selection_move_plan_clamps_to_rows() {
    assert_eq!(selection_move_plan(0, 3, 99), 2);
    assert_eq!(selection_move_plan(2, 3, -99), 0);
}

#[test]
fn selected_index_plan_clamps_to_rows() {
    assert_eq!(selected_index_plan(99, 3), 2);
    assert_eq!(selected_index_plan(0, 0), 0);
}

#[test]
fn unclamped_scroll_plan_updates_without_upper_clamp() {
    assert_eq!(unclamped_scroll_plan(4, 3), 7);
    assert_eq!(unclamped_scroll_plan(4, -99), 0);
}

#[test]
fn status_overlay_plans_construct_open_and_dismiss() {
    let StatusOverlayPlan::Open(state) = open_status_overlay_plan("Title", "Body") else {
        panic!("expected open plan");
    };
    let debug = format!("{state:?}");
    assert!(debug.contains("Title"));
    assert!(debug.contains("Body"));
    assert!(matches!(
        dismiss_status_overlay_plan(),
        StatusOverlayPlan::Dismiss
    ));
}

#[test]
fn role_resolution_status_overlay_plan_names_role() {
    let StatusOverlayPlan::Open(state) = role_resolution_status_overlay_plan("agent-smith") else {
        panic!("expected open plan");
    };
    let debug = format!("{state:?}");
    assert!(debug.contains("Resolving agent role"));
    assert!(debug.contains("agent-smith"));
}

#[test]
fn apply_status_overlay_plan_opens_and_dismisses() {
    let mut state = TestStatusOverlay::default();

    apply_status_overlay_plan(&mut state, open_status_overlay_plan("Title", "Body"));
    assert!(state.overlay.is_some());

    apply_status_overlay_plan(&mut state, dismiss_status_overlay_plan());
    assert!(state.overlay.is_none());
}

#[test]
fn apply_list_modal_plan_routes_modal_storage() {
    let mut state = TestListModal::default();

    apply_list_modal_plan(
        &mut state,
        open_container_info_modal_plan(
            crate::tui::components::container_info_surface::ContainerInfoState::new(
                "title",
                vec![
                    crate::tui::components::container_info_surface::ContainerInfoRow::new(
                        "label", "value",
                    ),
                ],
            ),
        ),
    );
    assert_eq!(state.opened, Some("container-info"));

    apply_list_modal_plan(
        &mut state,
        open_error_popup_modal_plan("Error title", "Error body"),
    );
    assert_eq!(state.opened, Some("error-popup"));

    apply_list_modal_plan(&mut state, dismiss_list_modal_plan());
    assert_eq!(state.opened, None);
}

#[test]
fn inline_picker_dismissal_plan_returns_requested_kind() {
    assert_eq!(
        inline_picker_dismissal_plan(InlinePickerDismissal::Agent),
        InlinePickerDismissal::Agent
    );
}

#[test]
fn apply_inline_picker_dismissal_plan_clears_requested_picker() {
    let mut state = TestInlinePickers::default();

    for dismissal in [
        InlinePickerDismissal::NewSession,
        InlinePickerDismissal::Role,
        InlinePickerDismissal::Agent,
        InlinePickerDismissal::Provider,
        InlinePickerDismissal::LaunchAccount,
    ] {
        apply_inline_picker_dismissal_plan(&mut state, dismissal);
    }

    assert_eq!(
        state.cleared,
        [
            "new-session",
            "role",
            "agent",
            "provider",
            "launch-provider",
        ]
    );
}

#[test]
fn shell_state_plans_return_normalized_values() {
    assert_eq!(
        list_scroll_focus_plan(Some(crate::tui::focus::MountScrollFocus::Workspace)),
        Some(crate::tui::focus::MountScrollFocus::Workspace)
    );
    assert!(list_names_focus_plan(true));
    let drag = crate::tui::split::DragState {
        anchor_pct: 30,
        anchor_x: 12,
    };
    assert_eq!(drag_state_plan(Some(drag)), Some(drag));
    assert_eq!(list_split_pct_plan(1), crate::tui::split::MIN_SPLIT_PCT);
    assert_eq!(list_split_pct_plan(99), crate::tui::split::MAX_SPLIT_PCT);
}

#[test]
fn shell_state_plan_application_updates_storage() {
    let mut state = TestListShell::default();
    let drag = crate::tui::split::DragState {
        anchor_pct: 30,
        anchor_x: 12,
    };

    apply_drag_state_plan(&mut state, drag_state_plan(Some(drag)));
    assert_eq!(state.drag, Some(drag));

    apply_drag_state_plan(&mut state, drag_state_plan(None));
    assert_eq!(state.drag, None);

    apply_list_split_pct_plan(&mut state, list_split_pct_plan(99));
    assert_eq!(state.split_pct, crate::tui::split::MAX_SPLIT_PCT);
}

#[test]
fn modal_scroll_targets_route_by_modal_facts() {
    assert_eq!(
        list_modal_key_target(true, true, true, true),
        ListModalKeyTarget::GithubPicker
    );
    assert_eq!(
        list_modal_key_target(false, true, true, true),
        ListModalKeyTarget::RolePicker
    );
    assert_eq!(
        list_modal_key_target(false, false, true, true),
        ListModalKeyTarget::ErrorPopup
    );
    assert_eq!(
        list_modal_key_target(false, false, false, true),
        ListModalKeyTarget::ContainerInfo
    );
    assert_eq!(
        list_modal_key_target(false, false, false, false),
        ListModalKeyTarget::Dismiss
    );

    assert_eq!(
        list_modal_scroll_target(true, true, true),
        ListModalScrollTarget::GithubPicker
    );
    assert_eq!(
        list_modal_scroll_target(false, true, true),
        ListModalScrollTarget::RolePicker
    );
    assert_eq!(
        list_modal_scroll_target(false, false, true),
        ListModalScrollTarget::OpPicker
    );
    assert_eq!(
        list_modal_scroll_target(false, false, false),
        ListModalScrollTarget::None
    );

    assert_eq!(
        shared_modal_scroll_target(true, true, true, true, true),
        SharedModalScrollTarget::WorkdirPick
    );
    assert_eq!(
        shared_modal_scroll_target(false, false, true, false, true),
        SharedModalScrollTarget::RolePicker
    );
    assert_eq!(
        shared_modal_scroll_target(false, false, false, false, true),
        SharedModalScrollTarget::OpPicker
    );
    assert_eq!(
        shared_modal_scroll_target(false, false, false, false, false),
        SharedModalScrollTarget::None
    );

    assert_eq!(
        settings_env_modal_scroll_target(true, true),
        SettingsModalScrollTarget::EnvOpPicker
    );
    assert_eq!(
        settings_env_modal_scroll_target(false, true),
        SettingsModalScrollTarget::EnvRolePicker
    );
    assert_eq!(
        settings_auth_modal_scroll_target(true),
        SettingsModalScrollTarget::AuthOpPicker
    );
    assert_eq!(
        global_mount_modal_scroll_target(true),
        SettingsModalScrollTarget::MountRolePicker
    );
}

#[test]
fn console_mouse_wheel_plan_routes_native_axes_and_shift_fallback() {
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollDown, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::Vertical(1)
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollUp, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::Vertical(-1)
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollRight, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::Horizontal {
            delta: 1,
            vertical_fallback: None,
        }
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::ScrollDown, KeyModifiers::SHIFT),
        ConsoleMouseWheelPlan::Horizontal {
            delta: 1,
            vertical_fallback: Some(1),
        }
    );
    assert_eq!(
        console_mouse_wheel_plan(MouseEventKind::Moved, KeyModifiers::NONE),
        ConsoleMouseWheelPlan::None
    );
}

#[test]
fn list_pre_render_focus_plan_handles_sidebar_liveness() {
    let missing_sidebar = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        false,
        false,
        false,
    );
    assert_eq!(missing_sidebar.list_scroll_focus, None);
    assert!(missing_sidebar.list_names_focused);

    let preview_missing_sidebar = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        true,
        false,
        false,
    );
    assert_eq!(preview_missing_sidebar.list_scroll_focus, None);
    assert!(!preview_missing_sidebar.list_names_focused);

    let stale_focus = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        true,
        true,
        false,
    );
    assert_eq!(stale_focus.list_scroll_focus, None);
    assert!(stale_focus.list_names_focused);

    let live_focus = list_pre_render_focus_plan(
        Some(crate::tui::focus::MountScrollFocus::Workspace),
        false,
        false,
        true,
        true,
    );
    assert_eq!(
        live_focus.list_scroll_focus,
        Some(crate::tui::focus::MountScrollFocus::Workspace)
    );
    assert!(!live_focus.list_names_focused);
}

#[test]
fn list_pre_render_scroll_reset_plan_resets_missing_scroll_slots() {
    assert_eq!(
        list_pre_render_scroll_reset_plan(false, false, false),
        ListPreRenderScrollResetPlan {
            reset_workspace: true,
            reset_global: true,
            reset_role_global: true,
            reset_roles: true,
        }
    );
    assert_eq!(
        list_pre_render_scroll_reset_plan(true, false, true),
        ListPreRenderScrollResetPlan {
            reset_workspace: false,
            reset_global: false,
            reset_role_global: true,
            reset_roles: false,
        }
    );
    assert_eq!(
        list_pre_render_scroll_reset_plan(true, true, false),
        ListPreRenderScrollResetPlan {
            reset_workspace: false,
            reset_global: false,
            reset_role_global: false,
            reset_roles: true,
        }
    );
}

#[test]
fn list_pre_render_plan_combines_scroll_reset_and_focus() {
    let plan = list_pre_render_plan(ListPreRenderFacts {
        list_scroll_focus: Some(crate::tui::focus::MountScrollFocus::Roles),
        list_names_focused: false,
        preview_focused: true,
        sidebar_available: true,
        focused_block_scrollable: false,
        role_global_available: false,
        roles_available: true,
    });

    assert_eq!(
        plan.scroll_reset,
        ListPreRenderScrollResetPlan {
            reset_workspace: false,
            reset_global: false,
            reset_role_global: true,
            reset_roles: false,
        }
    );
    assert_eq!(
        plan.focus,
        ListPreRenderFocusPlan {
            list_scroll_focus: None,
            list_names_focused: true,
        }
    );
}
