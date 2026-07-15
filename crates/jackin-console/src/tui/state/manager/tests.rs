// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Characterization tests for `ManagerState` trait seams (operator-console
//! INV-C1 / C7–C8 / C12–C13). Pure stage/poll helpers stay in `model/tests.rs`;
//! these pin the concrete manager impls screens and update paths call.

use jackin_config::AppConfig;
use tempfile::tempdir;
use termrock::components::ErrorPopupState;

use crate::tui::components::container_info_surface::ContainerInfoState;
use crate::tui::model::ConsoleManagerStageState;
use crate::tui::screens::workspaces::update::{
    PreviewFocusState, PreviewPaneCursorState, WorkspaceListSelectionState,
};
use crate::tui::state::{EditorState, ManagerStage, ManagerState, Modal};
use crate::tui::update::{
    InlinePickerDismissal, ListModalPlan, ListModalState, apply_inline_picker_dismissal_plan,
    apply_list_modal_plan,
};

fn fresh_manager() -> ManagerState<'static> {
    let tmp = tempdir().expect("tempdir");
    ManagerState::from_config(&AppConfig::default(), tmp.path())
}

/// INV-C1: `ManagerState` owns the active stage via `ConsoleManagerStageState`.
#[test]
fn manager_state_is_single_stage_owner() {
    let mut state = fresh_manager();
    assert!(matches!(state.stage, ManagerStage::List));

    state.set_manager_stage(ManagerStage::Editor(EditorState::new_create()));
    assert!(matches!(state.stage, ManagerStage::Editor(_)));

    state.set_manager_stage(ManagerStage::List);
    assert!(matches!(state.stage, ManagerStage::List));
}

/// INV-C7: list modals open/dismiss only through `ListModalState` on the manager.
#[test]
fn list_modals_open_through_list_modal_state() {
    let mut state = fresh_manager();
    assert!(state.list_modal.is_none());

    apply_list_modal_plan(
        &mut state,
        ListModalPlan::ErrorPopup(ErrorPopupState::new("title", "message")),
    );
    assert!(matches!(state.list_modal, Some(Modal::ErrorPopup { .. })));

    apply_list_modal_plan(
        &mut state,
        ListModalPlan::ContainerInfo(ContainerInfoState::new("c", vec![])),
    );
    assert!(matches!(
        state.list_modal,
        Some(Modal::ContainerInfo { .. })
    ));

    state.dismiss_list_modal();
    assert!(state.list_modal.is_none());
}

/// INV-C8: inline picker dismissal clears each picker slot independently.
#[test]
fn inline_picker_dismissal_clears_only_targeted_slot() {
    use crate::tui::components::agent_choice::AgentChoiceState;
    use jackin_core::RoleSelector;

    let mut state = fresh_manager();
    let role = RoleSelector::parse("agent-smith").expect("role");
    state.inline_agent_picker = Some((role, AgentChoiceState::new()));
    state.inline_role_picker = Some(crate::tui::components::role_picker::RolePickerState::new(
        vec![RoleSelector::parse("agent-smith").expect("role")],
    ));

    apply_inline_picker_dismissal_plan(&mut state, InlinePickerDismissal::Agent);
    assert!(state.inline_agent_picker.is_none());
    assert!(state.inline_role_picker.is_some());

    apply_inline_picker_dismissal_plan(&mut state, InlinePickerDismissal::Role);
    assert!(state.inline_role_picker.is_none());
}

/// INV-C12: workspace list selection + tree disclosure live on `ManagerState`.
#[test]
fn workspace_list_selection_and_disclosure_live_on_manager() {
    let mut state = fresh_manager();
    let before = state.selected;
    state.set_selected(before.saturating_add(1));
    assert_eq!(state.selected, before.saturating_add(1));

    // Disclosure methods are callable on the concrete manager (trait-split surface).
    state.collapse_current_dir();
    state.expand_current_dir();
    state.collapse_workspace(0);
    state.expand_workspace(0);
}

/// INV-C13: preview focus and pane cursor live on the manager, not screen locals.
#[test]
fn preview_focus_and_pane_cursor_live_on_manager() {
    let mut state = fresh_manager();
    assert!(!state.preview_focused);
    state.set_preview_focused(true);
    assert!(state.preview_focused);

    state.set_preview_pane_cursor("jk-agent-smith", 3);
    assert_eq!(state.preview_pane_cursor.get("jk-agent-smith"), Some(&3));
}

/// INV-C9 (manager hand-off): list stage has no pending poll results to drain.
#[test]
fn list_stage_poll_pending_handoffs_are_empty() {
    let mut state = fresh_manager();
    assert!(state.poll_pending_role_load().is_none());
    assert!(state.poll_pending_drift_check().is_none());
    assert!(state.poll_pending_isolation_cleanup().is_none());
    assert!(state.poll_pending_op_commit().is_none());
}
