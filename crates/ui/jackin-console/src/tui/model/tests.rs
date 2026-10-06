// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::launch_prompt::LaunchAccountPickerManagerState;
use super::launch_prompt::LaunchAgentPromptManagerState;
use super::launch_prompt::LaunchRolePromptManagerState;
use super::stage::ConsoleEditorFooterHeight;
use super::stage::ConsoleEditorModalPresence;
use super::stage::ConsoleManagerModalBlockPresence;
use super::stage::ConsolePendingDriftCheck;
use super::stage::ConsolePendingIsolationCleanup;
use super::stage::ConsolePendingOpCommit;
use super::stage::ConsolePendingOpCommitOrigin;
use super::stage::ConsolePendingRoleLoad;
use super::stage::ConsoleSettingsFooterHeight;
use super::stage::ConsoleSettingsModalPresence;
use std::path::PathBuf;

use jackin_config::MountIsolation;

use ratatui::layout::Rect;

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
};

use crate::tui::components::modal_overlay::{
    ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalContainerInfoState,
    ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState, ModalRolePickerState,
};

use crate::tui::debug::{
    ConsoleEditorDebugFacts, ConsoleModalDebugKind, ConsoleSettingsDebugFacts, ConsoleStageDebug,
    ModalDebugKind,
};

use super::{
    ConsoleAnimationTick, ConsoleApp, ConsoleAppStage, ConsoleCreatePreludeState,
    ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsoleManagerStage,
    ConsoleManagerStageRoute, ConsoleManagerStageState, ConsoleModal, ConsoleStageModalFacts,
    CreatePreludeCompletionStatus, CreatePreludeFileBrowserPlan, CreatePreludeKeyPlan,
    CreatePreludeMountDstChoicePlan, CreatePreludeTextInputDstPlan, CreatePreludeTextInputNamePlan,
    CreatePreludeWorkdirCancelPlan, CreatePreludeWorkdirPickPlan, apply_manager_stage,
    clear_pending_launch_plan, clear_pending_launch_role_plan, console_input_dispatch_plan,
    create_prelude_completion_status, create_prelude_file_browser_plan, create_prelude_key_plan,
    create_prelude_mount_dst_choice_plan, create_prelude_text_input_dst_plan,
    create_prelude_text_input_name_plan, create_prelude_wizard_state,
    create_prelude_workdir_cancel_plan, create_prelude_workdir_pick_plan,
    open_launch_account_picker_plan, open_launch_agent_prompt_plan, open_launch_role_prompt_plan,
    store_pending_launch_plan, take_pending_launch_and_role_plan, take_pending_launch_plan,
};

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
mod case_05;
