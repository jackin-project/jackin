// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::model::{
    ConsolePaintedInputOwner, ConsolePaintedInputOwnerFacts, console_painted_input_owner_plan,
};

fn dispatch_facts() -> ConsoleInputDispatchFacts {
    ConsoleInputDispatchFacts {
        keyboard_help_open: false,
        list_modal_open: false,
        inline_new_session_picker_open: false,
        inline_account_picker_open: false,
        launch_account_picker_open: false,
        inline_agent_picker_open: false,
        inline_role_picker_open: false,
        editor_modal_open: false,
        settings_error_popup_open: false,
        settings_mounts_modal_open: false,
        settings_env_modal_open: false,
        settings_auth_modal_open: false,
        create_prelude_modal_open: false,
        stage_route: ConsoleManagerStageRoute::List,
    }
}

#[test]
fn usage_surface_is_eligible_only_on_the_list_route() {
    assert!(console_usage_surface_painted(
        ConsoleManagerStageRoute::List,
        true
    ));
    assert!(!console_usage_surface_painted(
        ConsoleManagerStageRoute::List,
        false
    ));
    for route in [
        ConsoleManagerStageRoute::Editor,
        ConsoleManagerStageRoute::Settings,
        ConsoleManagerStageRoute::CreatePrelude,
        ConsoleManagerStageRoute::Workflows,
        ConsoleManagerStageRoute::ConfirmDelete,
        ConsoleManagerStageRoute::ConfirmInstancePurge,
    ] {
        assert!(
            !console_usage_surface_painted(route, true),
            "stale Usage cannot paint on {route:?}"
        );
    }
}

#[test]
fn painted_console_owner_orders_help_status_list_modal_and_usage() {
    let facts = ConsolePaintedInputOwnerFacts {
        dispatch: ConsoleInputDispatchFacts {
            keyboard_help_open: true,
            list_modal_open: true,
            ..dispatch_facts()
        },
        status_popup_painted: true,
        usage_surface_painted: true,
    };
    assert_eq!(
        console_painted_input_owner_plan(facts),
        ConsolePaintedInputOwner::KeyboardHelp
    );

    let facts = ConsolePaintedInputOwnerFacts {
        dispatch: ConsoleInputDispatchFacts {
            keyboard_help_open: false,
            ..facts.dispatch
        },
        ..facts
    };
    assert_eq!(
        console_painted_input_owner_plan(facts),
        ConsolePaintedInputOwner::StatusPopup
    );

    let facts = ConsolePaintedInputOwnerFacts {
        status_popup_painted: false,
        ..facts
    };
    assert_eq!(
        console_painted_input_owner_plan(facts),
        ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::ListModal)
    );

    let facts = ConsolePaintedInputOwnerFacts {
        dispatch: ConsoleInputDispatchFacts {
            list_modal_open: false,
            ..facts.dispatch
        },
        ..facts
    };
    assert_eq!(
        console_painted_input_owner_plan(facts),
        ConsolePaintedInputOwner::Usage
    );
}

#[test]
fn usage_owner_ignores_hidden_picker_facts_but_preserves_dispatch_when_unpainted() {
    let facts = ConsolePaintedInputOwnerFacts {
        dispatch: ConsoleInputDispatchFacts {
            inline_new_session_picker_open: true,
            inline_account_picker_open: true,
            launch_account_picker_open: true,
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
            ..dispatch_facts()
        },
        status_popup_painted: false,
        usage_surface_painted: true,
    };
    assert_eq!(
        console_painted_input_owner_plan(facts),
        ConsolePaintedInputOwner::Usage
    );

    let facts = ConsolePaintedInputOwnerFacts {
        usage_surface_painted: false,
        ..facts
    };
    assert_eq!(
        console_painted_input_owner_plan(facts),
        ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::InlineNewSessionPicker)
    );
}

#[test]
fn painted_owner_pointer_permission_tracks_manager_mouse_handlers() {
    assert!(!ConsolePaintedInputOwner::KeyboardHelp.allows_manager_pointer_actions());
    assert!(!ConsolePaintedInputOwner::StatusPopup.allows_manager_pointer_actions());
    assert!(!ConsolePaintedInputOwner::Usage.allows_manager_pointer_actions());
    for plan in [
        ConsoleInputDispatchPlan::KeyboardHelp,
        ConsoleInputDispatchPlan::InlineAccountPicker,
        ConsoleInputDispatchPlan::LaunchAccountPicker,
        ConsoleInputDispatchPlan::InlineNewSessionPicker,
        ConsoleInputDispatchPlan::InlineAgentPicker,
        ConsoleInputDispatchPlan::InlineRolePicker,
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::Workflows),
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::CreatePrelude),
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::ConfirmDelete),
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::ConfirmInstancePurge),
    ] {
        assert!(
            !ConsolePaintedInputOwner::Dispatch(plan).allows_manager_pointer_actions(),
            "pointer input must be inert for {plan:?}"
        );
    }
    for plan in [
        ConsoleInputDispatchPlan::ListModal,
        ConsoleInputDispatchPlan::EditorModal,
        ConsoleInputDispatchPlan::SettingsMountsModal,
        ConsoleInputDispatchPlan::CreatePreludeModal,
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::List),
    ] {
        assert!(
            ConsolePaintedInputOwner::Dispatch(plan).allows_manager_pointer_actions(),
            "current owner-specific pointer path must remain available for {plan:?}"
        );
    }
}
