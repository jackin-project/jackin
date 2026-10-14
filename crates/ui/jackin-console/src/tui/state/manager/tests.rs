// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::tui::components::ErrorPopupState;
use jackin_config::AppConfig;
use tempfile::tempdir;

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

#[test]
fn manager_recovery_export_is_bodyless() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, super::record_manager_recovery);

    export.force_flush();
    assert_eq!(export.event_count("operation.warn"), 1);
    assert!(export.contains_log_text("recovered_degradation"));
    for private in ["workspace", "selected", "index", "modal", "credential"] {
        assert!(!export.contains_log_text(private));
    }
}

fn fresh_manager() -> ManagerState<'static> {
    let tmp = tempdir().expect("tempdir");
    ManagerState::from_config(&AppConfig::default(), tmp.path())
}

#[test]
fn manager_state_is_single_stage_owner() {
    let mut state = fresh_manager();
    assert!(matches!(state.stage, ManagerStage::List));

    state.set_manager_stage(ManagerStage::Editor(EditorState::new_create()));
    assert!(matches!(state.stage, ManagerStage::Editor(_)));

    state.set_manager_stage(ManagerStage::List);
    assert!(matches!(state.stage, ManagerStage::List));
}

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

#[test]
fn workspace_list_selection_and_disclosure_live_on_manager() {
    let mut state = fresh_manager();
    let before = state.selected;
    state.set_selected(before.saturating_add(1));
    assert_eq!(state.selected, before.saturating_add(1));

    state.collapse_current_dir();
    state.expand_current_dir();
    state.collapse_workspace(0);
    state.expand_workspace(0);
}

#[test]
fn preview_focus_and_pane_cursor_live_on_manager() {
    let mut state = fresh_manager();
    assert!(!state.preview_focused);
    state.set_preview_focused(true);
    assert!(state.preview_focused);

    state.set_preview_pane_cursor("jk-agent-smith", 3);
    assert_eq!(state.preview_pane_cursor.get("jk-agent-smith"), Some(&3));
}

#[test]
fn list_stage_poll_pending_handoffs_are_empty() {
    let mut state = fresh_manager();
    assert!(state.poll_pending_role_load().is_none());
    assert!(state.poll_pending_drift_check().is_none());
    assert!(state.poll_pending_isolation_cleanup().is_none());
    assert!(state.poll_pending_op_commit().is_none());
}
