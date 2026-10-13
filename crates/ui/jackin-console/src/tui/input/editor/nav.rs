// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Editor scroll, selection, and navigation dispatch.

use crate::tui::screens::editor::model::{
    EditorFieldSelectionKeyPlan, EditorHorizontalScrollKeyPlan, EditorImmediateActionKeyPlan,
    EditorNavigationKeyPlan, EditorRoleHeaderExpansionKeyPlan, RoleHeaderExpansionPlan,
};

use crate::tui::state::ManagerState;
use crate::tui::state::update::{ManagerMessage, update_manager};

pub(crate) fn dispatch_manager(state: &mut ManagerState<'_>, message: ManagerMessage) {
    update_manager(state, message);
}

pub(crate) fn dispatch_editor_horizontal_scroll(
    state: &mut ManagerState<'_>,
    plan: EditorHorizontalScrollKeyPlan,
    term_width: u16,
) {
    match plan {
        EditorHorizontalScrollKeyPlan::WorkspaceMounts {
            delta,
            content_width,
        } => dispatch_manager(
            state,
            ManagerMessage::ScrollEditorWorkspaceMountsHorizontal {
                delta,
                term_width,
                content_width,
            },
        ),
        EditorHorizontalScrollKeyPlan::TabContent {
            delta,
            content_width,
        } => dispatch_manager(
            state,
            ManagerMessage::ScrollEditorTabHorizontal {
                delta,
                term_width,
                content_width,
            },
        ),
    }
}

pub(crate) fn dispatch_editor_field_selection(
    state: &mut ManagerState<'_>,
    plan: EditorFieldSelectionKeyPlan,
) {
    dispatch_manager(
        state,
        ManagerMessage::MoveEditorFieldSelection {
            delta: plan.delta,
            max_row: plan.max_row,
            skipped_rows: plan.skipped_rows,
            term: plan.term,
            footer_h: plan.footer_h,
        },
    );
}

pub(crate) fn dispatch_editor_navigation(
    state: &mut ManagerState<'_>,
    plan: EditorNavigationKeyPlan,
) -> bool {
    match plan {
        EditorNavigationKeyPlan::MoveTab {
            delta,
            focus_tab_bar,
        } => {
            dispatch_manager(
                state,
                ManagerMessage::MoveEditorTab {
                    delta,
                    focus_tab_bar,
                },
            );
            true
        }
        EditorNavigationKeyPlan::FocusContent => {
            dispatch_manager(state, ManagerMessage::FocusEditorContent);
            true
        }
        EditorNavigationKeyPlan::FocusTabBar => {
            dispatch_manager(state, ManagerMessage::FocusEditorTabBar);
            true
        }
        EditorNavigationKeyPlan::NotNavigation => false,
    }
}

pub(crate) fn dispatch_editor_immediate_action(
    state: &mut ManagerState<'_>,
    plan: EditorImmediateActionKeyPlan,
) -> bool {
    match plan {
        EditorImmediateActionKeyPlan::ToggleGeneralSelected => {
            dispatch_manager(state, ManagerMessage::ToggleEditorGeneralSelected);
            true
        }
        EditorImmediateActionKeyPlan::ToggleMountReadonlySelected => {
            dispatch_manager(state, ManagerMessage::ToggleEditorMountReadonlySelected);
            true
        }
        EditorImmediateActionKeyPlan::ToggleSecretMask { scope, key } => {
            dispatch_manager(state, ManagerMessage::ToggleEditorSecretMask { scope, key });
            true
        }
        EditorImmediateActionKeyPlan::NotImmediateAction => false,
    }
}

pub(crate) fn dispatch_editor_role_header_expansion(
    state: &mut ManagerState<'_>,
    plan: EditorRoleHeaderExpansionKeyPlan,
) {
    match plan {
        EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::Set {
            role,
            expanded,
        }) => {
            dispatch_manager(
                state,
                ManagerMessage::SetEditorSecretsRoleExpanded { role, expanded },
            );
        }
        EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::HeaderNoop) => {}
        EditorRoleHeaderExpansionKeyPlan::Secrets(RoleHeaderExpansionPlan::NotHeader)
        | EditorRoleHeaderExpansionKeyPlan::NotRoleHeaderTab => {}
    }
}
