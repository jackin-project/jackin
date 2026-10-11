// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) struct TestConfirm;

pub(super) struct TestEditor {
    pub(super) modal_open: bool,
    pub(super) footer_height: u16,
}

impl ConsoleEditorModalPresence for TestEditor {
    fn editor_modal_open(&self) -> bool {
        self.modal_open
    }
}

impl ConsoleEditorFooterHeight for TestEditor {
    fn editor_cached_footer_height(&self) -> u16 {
        self.footer_height
    }
}

impl ConsoleEditorDebugFacts for TestEditor {
    fn editor_stage_debug(&self) -> ConsoleStageDebug {
        ConsoleStageDebug::Editor {
            mode: "TestMode".to_owned(),
            tab: "TestTab".to_owned(),
            field: "TestField".to_owned(),
            modal: self.modal_open.then_some(ModalDebugKind::TextInput),
        }
    }
}

pub(super) struct TestSettings {
    pub(super) facts: ConsoleStageModalFacts,
    pub(super) footer_height: u16,
}

impl ConsoleSettingsModalPresence for TestSettings {
    fn settings_modal_facts(&self) -> ConsoleStageModalFacts {
        self.facts
    }
}

impl ConsoleSettingsFooterHeight for TestSettings {
    fn settings_cached_footer_height(&self) -> u16 {
        self.footer_height
    }
}

impl ConsoleSettingsDebugFacts for TestSettings {
    fn settings_stage_debug(&self) -> ConsoleStageDebug {
        ConsoleStageDebug::Settings {
            tab: "Mounts".to_owned(),
            selected: 2,
            modal: None,
        }
    }
}

pub(super) struct TestRoleLoad {
    pub(super) pending: Option<u8>,
}

impl ConsolePendingRoleLoad for TestRoleLoad {
    type PendingRoleLoad = u8;

    fn poll_pending_role_load(&mut self) -> Option<(Self::PendingRoleLoad, anyhow::Result<()>)> {
        self.pending.take().map(|pending| (pending, Ok(())))
    }
}

pub(super) struct TestDriftCheck {
    pub(super) pending: Option<(u8, &'static str)>,
}

impl ConsolePendingDriftCheck for TestDriftCheck {
    type PendingDriftCheck = u8;
    type DriftDetection = &'static str;

    fn poll_pending_drift_check(
        &mut self,
    ) -> Option<(
        Self::PendingDriftCheck,
        anyhow::Result<Self::DriftDetection>,
    )> {
        self.pending
            .take()
            .map(|(pending, detection)| (pending, Ok(detection)))
    }
}

pub(super) struct TestIsolationCleanup {
    pub(super) pending: Option<u8>,
}

impl ConsolePendingIsolationCleanup for TestIsolationCleanup {
    type PendingIsolationCleanup = u8;

    fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(Self::PendingIsolationCleanup, anyhow::Result<()>)> {
        self.pending.take().map(|pending| (pending, Ok(())))
    }
}

pub(super) struct TestOpCommit {
    pub(super) pending: Option<(u8, anyhow::Result<()>)>,
}

impl ConsolePendingOpCommit for TestOpCommit {
    type OpRef = u8;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
        self.pending.take()
    }
}

pub(super) struct TestDebugModal;

impl ConsoleModalDebugKind for TestDebugModal {
    fn modal_debug_kind(&self) -> ModalDebugKind {
        ModalDebugKind::ErrorPopup
    }
}

#[derive(Debug)]
pub(super) struct TestManager {
    pub(super) list_modal_open: bool,
    pub(super) editor_modal_open: bool,
}

impl ConsoleManagerModalBlockPresence for TestManager {
    fn list_modal_open(&self) -> bool {
        self.list_modal_open
    }

    fn editor_modal_open(&self) -> bool {
        self.editor_modal_open
    }
}

#[derive(Debug, Default)]
pub(super) struct TestLaunchPromptManager {
    pub(super) opened_role: Option<&'static str>,
    pub(super) picker_choices: Vec<jackin_core::Agent>,
    pub(super) role_prompt_cleared: bool,
    pub(super) role_picker_keys: Vec<&'static str>,
    pub(super) role_picker_selected: Option<usize>,
    pub(super) role_picker_confirm_label: String,
    pub(super) account_picker_role: Option<TestPromptRole>,
    pub(super) account_picker_agent: Option<jackin_core::Agent>,
    pub(super) account_picker_providers: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestPromptRole(pub(super) &'static str);

impl crate::tui::components::role_picker::RoleChoice for TestPromptRole {
    fn key(&self) -> String {
        self.0.to_owned()
    }
}

impl LaunchAgentPromptManagerState<&'static str, jackin_core::Agent> for TestLaunchPromptManager {
    fn open_launch_agent_prompt(
        &mut self,
        role: &'static str,
        picker: crate::tui::components::agent_choice::AgentChoiceState<jackin_core::Agent>,
    ) {
        self.opened_role = Some(role);
        self.picker_choices = picker.choices;
    }

    fn clear_launch_role_prompt(&mut self) {
        self.role_prompt_cleared = true;
    }
}

impl LaunchRolePromptManagerState<TestPromptRole> for TestLaunchPromptManager {
    fn open_launch_role_prompt(
        &mut self,
        picker: crate::tui::components::role_picker::RolePickerState<TestPromptRole>,
    ) {
        self.role_picker_keys = picker.roles.iter().map(|role| role.0).collect();
        self.role_picker_selected = picker.list_state.selected().copied();
        self.role_picker_confirm_label = picker.confirm_label;
    }
}

impl LaunchAccountPickerManagerState<TestPromptRole, jackin_core::Agent, &'static str>
    for TestLaunchPromptManager
{
    fn open_launch_account_picker(
        &mut self,
        picker: crate::tui::components::account_picker::AccountPickerState<
            TestPromptRole,
            jackin_core::Agent,
            &'static str,
        >,
    ) {
        let providers = picker.providers().to_vec();
        self.account_picker_role = Some(picker.context);
        self.account_picker_agent = Some(picker.agent);
        self.account_picker_providers = providers;
    }
}

impl ModalConfirmState for TestConfirm {
    fn width_pct(&self) -> u16 {
        42
    }

    fn required_height(&self) -> u16 {
        9
    }
}

#[derive(Default)]
pub(super) struct TestStageState {
    pub(super) stage: Option<ConsoleManagerStage<(), (), ()>>,
}

impl ConsoleManagerStageState<ConsoleManagerStage<(), (), ()>> for TestStageState {
    fn set_manager_stage(&mut self, stage: ConsoleManagerStage<(), (), ()>) {
        self.stage = Some(stage);
    }
}

pub(super) struct TestGithubPicker(pub(super) usize);

impl ModalGithubPickerState for TestGithubPicker {
    fn choice_len(&self) -> usize {
        self.0
    }
}

pub(super) struct TestConfirmSave;

impl ModalConfirmSaveState for TestConfirmSave {
    fn required_height(&self) -> u16 {
        12
    }
}

impl ModalConfirmSaveFooterState for TestConfirmSave {
    fn footer_mode(&self) -> ModalFooterMode {
        ModalFooterMode::ConfirmSave {
            scroll_axes: termrock::scroll::ScrollAxes::none(),
        }
    }
}

pub(super) struct TestError;

impl ModalErrorPopupState for TestError {
    fn required_height(&self, _inner_width: u16, _max_rows: u16) -> u16 {
        14
    }
}

pub(super) struct TestContainerInfo;

impl ModalContainerInfoState for TestContainerInfo {
    fn required_height(&self) -> u16 {
        15
    }
}

impl ModalContainerInfoFooterState for TestContainerInfo {
    fn content_width(&self) -> usize {
        80
    }

    fn content_height(&self) -> usize {
        40
    }
}

pub(super) struct TestOpPicker(pub(super) bool);

impl ModalOpPickerState for TestOpPicker {
    fn has_naming_stage_input(&self) -> bool {
        self.0
    }
}

impl ConsoleAnimationTick for TestOpPicker {
    fn tick_active_animation(&mut self) -> bool {
        self.0
    }
}

impl ModalOpPickerFooterState for TestOpPicker {
    fn footer_mode(&self, include_refresh: bool) -> ModalFooterMode {
        ModalFooterMode::FilteredPicker {
            include_refresh,
            include_collapse: false,
        }
    }
}

pub(super) struct TestRolePicker(pub(super) usize);

impl ModalRolePickerState for TestRolePicker {
    fn filtered_len(&self) -> usize {
        self.0
    }
}

pub(super) struct TestAuthForm;

impl ModalAuthFormState for TestAuthForm {
    fn required_height(&self) -> u16 {
        13
    }
}

impl ModalAuthFormFooterState<()> for TestAuthForm {
    fn footer_mode(&self, _focus: (), can_generate_token: bool) -> ModalFooterMode {
        ModalFooterMode::AuthForm {
            focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
            shows_source_folder: false,
            shows_credential_block: false,
            can_generate_token,
        }
    }
}

pub(super) struct TestFileBrowser;

impl ModalFileBrowserFooterState for TestFileBrowser {
    fn footer_items(&self) -> Vec<termrock::widgets::HintSpan<'static>> {
        vec![termrock::widgets::HintSpan::Text("file")]
    }
}

pub(super) type RectTestModal = ConsoleModal<
    (),
    (),
    (),
    TestFileBrowser,
    (),
    (),
    (),
    TestConfirm,
    (),
    TestGithubPicker,
    TestConfirmSave,
    TestError,
    TestContainerInfo,
    (),
    TestOpPicker,
    TestRolePicker,
    (),
    (),
    (),
    TestAuthForm,
    (),
    (),
>;

pub(super) struct TestAnimationTick(pub(super) bool);

impl ConsoleAnimationTick for TestAnimationTick {
    fn tick_active_animation(&mut self) -> bool {
        self.0
    }
}
