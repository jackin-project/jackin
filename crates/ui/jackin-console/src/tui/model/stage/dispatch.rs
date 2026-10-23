// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::ConsoleManagerStageRoute;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInputDispatchPlan {
    KeyboardHelp,
    ListModal,
    InlineNewSessionPicker,
    InlineAccountPicker,
    LaunchAccountPicker,
    InlineAgentPicker,
    InlineRolePicker,
    EditorModal,
    SettingsErrorPopup,
    SettingsMountsModal,
    SettingsEnvDialog,
    SettingsAuthDialog,
    CreatePreludeModal,
    Stage(ConsoleManagerStageRoute),
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Thirteen orthogonal console-modal-open flags (keyboard_help, \
              list_modal, inline pickers, editor_modal, settings pickers, \
              create_prelude_modal) — each is an independent picker-open \
              signal the input dispatch planner reads individually to pick \
              the right dispatch arm. Named-field reads match the per-modal \
              dispatch routing idiom."
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsoleInputDispatchFacts {
    pub keyboard_help_open: bool,
    pub list_modal_open: bool,
    pub inline_new_session_picker_open: bool,
    pub inline_account_picker_open: bool,
    pub launch_account_picker_open: bool,
    pub inline_agent_picker_open: bool,
    pub inline_role_picker_open: bool,
    pub editor_modal_open: bool,
    pub settings_error_popup_open: bool,
    pub settings_mounts_modal_open: bool,
    pub settings_env_modal_open: bool,
    pub settings_auth_modal_open: bool,
    pub create_prelude_modal_open: bool,
    pub stage_route: ConsoleManagerStageRoute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsolePaintedInputOwner {
    KeyboardHelp,
    StatusPopup,
    Usage,
    Dispatch(ConsoleInputDispatchPlan),
}

impl ConsolePaintedInputOwner {
    #[must_use]
    pub const fn allows_manager_pointer_actions(self) -> bool {
        match self {
            Self::KeyboardHelp | Self::StatusPopup | Self::Usage => false,
            Self::Dispatch(
                ConsoleInputDispatchPlan::KeyboardHelp
                | ConsoleInputDispatchPlan::InlineNewSessionPicker
                | ConsoleInputDispatchPlan::InlineAccountPicker
                | ConsoleInputDispatchPlan::LaunchAccountPicker
                | ConsoleInputDispatchPlan::InlineAgentPicker
                | ConsoleInputDispatchPlan::InlineRolePicker
                | ConsoleInputDispatchPlan::Stage(
                    ConsoleManagerStageRoute::CreatePrelude
                    | ConsoleManagerStageRoute::Workflows
                    | ConsoleManagerStageRoute::ConfirmDelete
                    | ConsoleManagerStageRoute::ConfirmInstancePurge,
                ),
            ) => false,
            Self::Dispatch(
                ConsoleInputDispatchPlan::ListModal
                | ConsoleInputDispatchPlan::EditorModal
                | ConsoleInputDispatchPlan::SettingsErrorPopup
                | ConsoleInputDispatchPlan::SettingsMountsModal
                | ConsoleInputDispatchPlan::SettingsEnvDialog
                | ConsoleInputDispatchPlan::SettingsAuthDialog
                | ConsoleInputDispatchPlan::CreatePreludeModal
                | ConsoleInputDispatchPlan::Stage(
                    ConsoleManagerStageRoute::List
                    | ConsoleManagerStageRoute::Editor
                    | ConsoleManagerStageRoute::Settings,
                ),
            ) => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsolePaintedInputOwnerFacts {
    pub dispatch: ConsoleInputDispatchFacts,
    pub status_popup_painted: bool,
    pub usage_surface_painted: bool,
}

#[must_use]
pub const fn console_usage_surface_painted(
    route: ConsoleManagerStageRoute,
    usage_visible: bool,
) -> bool {
    matches!(route, ConsoleManagerStageRoute::List) && usage_visible
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Seven orthogonal stage-modal-open flags (editor_modal, settings \
              pickers, create_prelude_modal, destructive_confirm) — each is an \
              independent picker-open signal the stage-modal resolver reads to \
              build the visible stage modal set. Named-field reads match the \
              per-picker stage-modal routing idiom."
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleStageModalFacts {
    pub editor_modal_open: bool,
    pub settings_error_popup_open: bool,
    pub settings_mounts_modal_open: bool,
    pub settings_env_modal_open: bool,
    pub settings_auth_modal_open: bool,
    pub create_prelude_modal_open: bool,
    pub destructive_confirm_open: bool,
}

#[must_use]
pub const fn console_input_dispatch_plan(
    facts: ConsoleInputDispatchFacts,
) -> ConsoleInputDispatchPlan {
    if facts.keyboard_help_open {
        return ConsoleInputDispatchPlan::KeyboardHelp;
    }
    if facts.list_modal_open {
        return ConsoleInputDispatchPlan::ListModal;
    }
    if facts.inline_new_session_picker_open {
        return ConsoleInputDispatchPlan::InlineNewSessionPicker;
    }
    if facts.inline_account_picker_open {
        return ConsoleInputDispatchPlan::InlineAccountPicker;
    }
    if facts.launch_account_picker_open {
        return ConsoleInputDispatchPlan::LaunchAccountPicker;
    }
    if facts.inline_agent_picker_open {
        return ConsoleInputDispatchPlan::InlineAgentPicker;
    }
    if facts.inline_role_picker_open {
        return ConsoleInputDispatchPlan::InlineRolePicker;
    }
    if facts.editor_modal_open {
        return ConsoleInputDispatchPlan::EditorModal;
    }
    if facts.settings_error_popup_open {
        return ConsoleInputDispatchPlan::SettingsErrorPopup;
    }
    if facts.settings_mounts_modal_open {
        return ConsoleInputDispatchPlan::SettingsMountsModal;
    }
    if facts.settings_env_modal_open {
        return ConsoleInputDispatchPlan::SettingsEnvDialog;
    }
    if facts.settings_auth_modal_open {
        return ConsoleInputDispatchPlan::SettingsAuthDialog;
    }
    if facts.create_prelude_modal_open {
        return ConsoleInputDispatchPlan::CreatePreludeModal;
    }
    ConsoleInputDispatchPlan::Stage(facts.stage_route)
}

#[must_use]
pub const fn console_painted_input_owner_plan(
    facts: ConsolePaintedInputOwnerFacts,
) -> ConsolePaintedInputOwner {
    if facts.dispatch.keyboard_help_open {
        return ConsolePaintedInputOwner::KeyboardHelp;
    }
    if facts.status_popup_painted {
        return ConsolePaintedInputOwner::StatusPopup;
    }
    if facts.dispatch.list_modal_open {
        return ConsolePaintedInputOwner::Dispatch(ConsoleInputDispatchPlan::ListModal);
    }
    if facts.usage_surface_painted {
        return ConsolePaintedInputOwner::Usage;
    }

    let mut dispatch = facts.dispatch;
    dispatch.keyboard_help_open = false;
    dispatch.list_modal_open = false;
    ConsolePaintedInputOwner::Dispatch(console_input_dispatch_plan(dispatch))
}
