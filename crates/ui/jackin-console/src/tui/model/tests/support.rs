// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
struct TestConfirm;

struct TestEditor {
    modal_open: bool,
    footer_height: u16,
}

impl super::ConsoleEditorModalPresence for TestEditor {
    fn editor_modal_open(&self) -> bool {
        self.modal_open
    }
}

impl super::ConsoleEditorFooterHeight for TestEditor {
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

struct TestSettings {
    facts: ConsoleStageModalFacts,
    footer_height: u16,
}

impl super::ConsoleSettingsModalPresence for TestSettings {
    fn settings_modal_facts(&self) -> ConsoleStageModalFacts {
        self.facts
    }
}

impl super::ConsoleSettingsFooterHeight for TestSettings {
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

struct TestRoleLoad {
    pending: Option<u8>,
}

impl super::ConsolePendingRoleLoad for TestRoleLoad {
    type PendingRoleLoad = u8;

    fn poll_pending_role_load(&mut self) -> Option<(Self::PendingRoleLoad, anyhow::Result<()>)> {
        self.pending.take().map(|pending| (pending, Ok(())))
    }
}

struct TestDriftCheck {
    pending: Option<(u8, &'static str)>,
}

impl super::ConsolePendingDriftCheck for TestDriftCheck {
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

struct TestIsolationCleanup {
    pending: Option<u8>,
}

impl super::ConsolePendingIsolationCleanup for TestIsolationCleanup {
    type PendingIsolationCleanup = u8;

    fn poll_pending_isolation_cleanup(
        &mut self,
    ) -> Option<(Self::PendingIsolationCleanup, anyhow::Result<()>)> {
        self.pending.take().map(|pending| (pending, Ok(())))
    }
}

struct TestOpCommit {
    pending: Option<(u8, anyhow::Result<()>)>,
}

impl super::ConsolePendingOpCommit for TestOpCommit {
    type OpRef = u8;

    fn poll_pending_op_commit(&mut self) -> Option<(Self::OpRef, anyhow::Result<()>)> {
        self.pending.take()
    }
}

struct TestDebugModal;

impl ConsoleModalDebugKind for TestDebugModal {
    fn modal_debug_kind(&self) -> ModalDebugKind {
        ModalDebugKind::ErrorPopup
    }
}

#[derive(Debug)]
struct TestManager {
    list_modal_open: bool,
    editor_modal_open: bool,
}

impl super::ConsoleManagerModalBlockPresence for TestManager {
    fn list_modal_open(&self) -> bool {
        self.list_modal_open
    }

    fn editor_modal_open(&self) -> bool {
        self.editor_modal_open
    }
}

#[derive(Debug, Default)]
struct TestLaunchPromptManager {
    opened_role: Option<&'static str>,
    picker_choices: Vec<jackin_core::Agent>,
    role_prompt_cleared: bool,
    role_picker_keys: Vec<&'static str>,
    role_picker_selected: Option<usize>,
    role_picker_confirm_label: String,
    account_picker_role: Option<TestPromptRole>,
    account_picker_agent: Option<jackin_core::Agent>,
    account_picker_providers: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestPromptRole(&'static str);

impl crate::tui::components::role_picker::RoleChoice for TestPromptRole {
    fn key(&self) -> String {
        self.0.to_owned()
    }
}

impl super::LaunchAgentPromptManagerState<&'static str, jackin_core::Agent>
    for TestLaunchPromptManager
{
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

impl super::LaunchRolePromptManagerState<TestPromptRole> for TestLaunchPromptManager {
    fn open_launch_role_prompt(
        &mut self,
        picker: crate::tui::components::role_picker::RolePickerState<TestPromptRole>,
    ) {
        self.role_picker_keys = picker.roles.iter().map(|role| role.0).collect();
        self.role_picker_selected = picker.list_state.selected().copied();
        self.role_picker_confirm_label = picker.confirm_label;
    }
}

impl super::LaunchAccountPickerManagerState<TestPromptRole, jackin_core::Agent, &'static str>
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
struct TestStageState {
    stage: Option<ConsoleManagerStage<(), (), ()>>,
}

impl ConsoleManagerStageState<ConsoleManagerStage<(), (), ()>> for TestStageState {
    fn set_manager_stage(&mut self, stage: ConsoleManagerStage<(), (), ()>) {
        self.stage = Some(stage);
    }
}

struct TestGithubPicker(usize);

impl ModalGithubPickerState for TestGithubPicker {
    fn choice_len(&self) -> usize {
        self.0
    }
}

struct TestConfirmSave;

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

struct TestError;

impl ModalErrorPopupState for TestError {
    fn required_height(&self, _inner_width: u16, _max_rows: u16) -> u16 {
        14
    }
}

struct TestContainerInfo;

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

struct TestOpPicker(bool);

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

struct TestRolePicker(usize);

impl ModalRolePickerState for TestRolePicker {
    fn filtered_len(&self) -> usize {
        self.0
    }
}

struct TestAuthForm;

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

struct TestFileBrowser;

impl ModalFileBrowserFooterState for TestFileBrowser {
    fn footer_items(&self) -> Vec<termrock::widgets::HintSpan<'static>> {
        vec![termrock::widgets::HintSpan::Text("file")]
    }
}

type RectTestModal = ConsoleModal<
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

struct TestAnimationTick(bool);

impl ConsoleAnimationTick for TestAnimationTick {
    fn tick_active_animation(&mut self) -> bool {
        self.0
    }
}

