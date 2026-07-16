// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use jackin_config::MountIsolation;
use ratatui::layout::Rect;

use crate::tui::components::footer_hints::{
    ModalAuthFormFooterState, ModalConfirmSaveFooterState, ModalContainerInfoFooterState,
    ModalFileBrowserFooterState, ModalFooterMode, ModalOpPickerFooterState,
};
use crate::tui::components::modal_rects::{
    ModalAuthFormState, ModalConfirmSaveState, ModalConfirmState, ModalContainerInfoState,
    ModalErrorPopupState, ModalGithubPickerState, ModalOpPickerState, ModalRectMode,
    ModalRolePickerState,
};
use crate::tui::debug::{
    ConsoleEditorDebugFacts, ConsoleModalDebugKind, ConsoleSettingsDebugFacts, ConsoleStageDebug,
    ModalDebugKind,
};
use crate::tui::screens::editor::model::CreateStep;

use super::{
    ConsoleAnimationTick, ConsoleApp, ConsoleAppStage, ConsoleCreatePreludeState,
    ConsoleInputDispatchFacts, ConsoleInputDispatchPlan, ConsoleManagerStage,
    ConsoleManagerStageRoute, ConsoleManagerStageState, ConsoleModal, ConsoleStageModalFacts,
    CreatePreludeCompletionStatus, CreatePreludeFileBrowserPlan, CreatePreludeKeyPlan,
    CreatePreludeModalStep, CreatePreludeMountDstChoicePlan, CreatePreludeTextInputDstPlan,
    CreatePreludeTextInputNamePlan, CreatePreludeWorkdirCancelPlan, CreatePreludeWorkdirPickPlan,
    apply_manager_stage, clear_pending_launch_plan, clear_pending_launch_role_plan,
    console_input_dispatch_plan, create_prelude_completion_status,
    create_prelude_file_browser_plan, create_prelude_key_plan, create_prelude_modal_step,
    create_prelude_mount_dst_choice_plan, create_prelude_text_input_dst_plan,
    create_prelude_text_input_name_plan, create_prelude_workdir_cancel_plan,
    create_prelude_workdir_pick_plan, open_launch_agent_prompt_plan,
    open_launch_provider_picker_plan, open_launch_role_prompt_plan, store_pending_launch_plan,
    take_pending_launch_and_role_plan, take_pending_launch_plan,
};

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

struct TestTokenDrain {
    pending: Option<u8>,
}

impl super::ConsolePendingTokenGenerate for TestTokenDrain {
    type PendingTokenGenerate = u8;

    fn take_pending_token_generate(&mut self) -> Option<Self::PendingTokenGenerate> {
        self.pending.take()
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
    provider_picker_role: Option<TestPromptRole>,
    provider_picker_agent: Option<jackin_core::Agent>,
    provider_picker_providers: Vec<&'static str>,
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

impl super::LaunchProviderPickerManagerState<TestPromptRole, jackin_core::Agent, &'static str>
    for TestLaunchPromptManager
{
    fn open_launch_provider_picker(
        &mut self,
        picker: crate::tui::components::provider_picker::ProviderPickerState<
            TestPromptRole,
            jackin_core::Agent,
            &'static str,
        >,
    ) {
        let providers = picker.providers().to_vec();
        self.provider_picker_role = Some(picker.context);
        self.provider_picker_agent = Some(picker.agent);
        self.provider_picker_providers = providers;
    }
}

#[test]
fn open_launch_agent_prompt_plan_updates_app_and_manager() {
    let mut app: ConsoleApp<TestLaunchPromptManager, (), &'static str, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
        (),
        false,
    );

    open_launch_agent_prompt_plan(&mut app, "architect", vec![jackin_core::Agent::Claude]);

    assert_eq!(app.pending_launch_role, Some("architect"));
    let ConsoleAppStage::Manager(manager) = app.stage;
    assert_eq!(manager.opened_role, Some("architect"));
    assert_eq!(manager.picker_choices, vec![jackin_core::Agent::Claude]);
    assert!(manager.role_prompt_cleared);
}

#[test]
fn open_launch_role_prompt_plan_updates_app_and_manager() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );

    open_launch_role_prompt_plan(
        &mut app,
        "workspace-input",
        vec![TestPromptRole("architect"), TestPromptRole("reviewer")],
        Some(1),
    );

    assert_eq!(app.pending_launch, Some("workspace-input"));
    assert_eq!(app.pending_launch_role, None);
    let ConsoleAppStage::Manager(manager) = app.stage;
    assert_eq!(manager.role_picker_keys, vec!["architect", "reviewer"]);
    assert_eq!(manager.role_picker_selected, Some(1));
    assert_eq!(manager.role_picker_confirm_label, "launch");
}

#[test]
fn clear_pending_launch_plan_clears_launch_state() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );
    app.pending_launch = Some("workspace-input");
    app.pending_launch_role = Some(TestPromptRole("architect"));

    clear_pending_launch_plan(&mut app);

    assert_eq!(app.pending_launch, None);
    assert_eq!(app.pending_launch_role, None);
}

#[test]
fn store_pending_launch_plan_sets_launch_input() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );

    store_pending_launch_plan(&mut app, "workspace-input");

    assert_eq!(app.pending_launch, Some("workspace-input"));
}

#[test]
fn clear_pending_launch_role_plan_clears_only_role() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );
    app.pending_launch = Some("workspace-input");
    app.pending_launch_role = Some(TestPromptRole("architect"));

    clear_pending_launch_role_plan(&mut app);

    assert_eq!(app.pending_launch, Some("workspace-input"));
    assert_eq!(app.pending_launch_role, None);
}

#[test]
fn take_pending_launch_plan_takes_input() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );
    app.pending_launch = Some("workspace-input");

    assert_eq!(take_pending_launch_plan(&mut app), Some("workspace-input"));
    assert_eq!(app.pending_launch, None);
}

#[test]
fn take_pending_launch_and_role_plan_takes_pair() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );
    app.pending_launch = Some("workspace-input");
    app.pending_launch_role = Some(TestPromptRole("architect"));

    assert_eq!(
        take_pending_launch_and_role_plan(&mut app),
        Some(("workspace-input", TestPromptRole("architect")))
    );
    assert_eq!(app.pending_launch, None);
    assert_eq!(app.pending_launch_role, None);
}

#[test]
fn open_launch_provider_picker_plan_updates_app_and_manager() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );

    open_launch_provider_picker_plan(
        &mut app,
        "workspace-input",
        TestPromptRole("architect"),
        jackin_core::Agent::Claude,
        vec!["anthropic", "zai"],
    );

    assert_eq!(app.pending_launch, Some("workspace-input"));
    assert_eq!(app.pending_launch_role, Some(TestPromptRole("architect")));
    let ConsoleAppStage::Manager(manager) = app.stage;
    assert_eq!(
        manager.provider_picker_role,
        Some(TestPromptRole("architect"))
    );
    assert_eq!(
        manager.provider_picker_agent,
        Some(jackin_core::Agent::Claude)
    );
    assert_eq!(manager.provider_picker_providers, vec!["anthropic", "zai"]);
}

impl ModalConfirmState for TestConfirm {
    fn width_pct(&self) -> u16 {
        42
    }

    fn required_height(&self) -> u16 {
        9
    }
}

#[test]
fn console_app_base_surface_unblocked_respects_modal_blockers() {
    let mut app: ConsoleApp<TestManager, (), (), ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(TestManager {
            list_modal_open: false,
            editor_modal_open: false,
        }),
        (),
        false,
    );

    assert!(app.base_surface_unblocked());

    app.open_quit_confirm();
    assert!(!app.base_surface_unblocked());

    app.dismiss_quit_confirm();
    app.stage = ConsoleAppStage::Manager(TestManager {
        list_modal_open: true,
        editor_modal_open: false,
    });
    assert!(!app.base_surface_unblocked());

    app.stage = ConsoleAppStage::Manager(TestManager {
        list_modal_open: false,
        editor_modal_open: true,
    });
    assert!(!app.base_surface_unblocked());
}

#[test]
fn console_app_quit_confirm_key_dismisses_dialog() {
    let mut app: ConsoleApp<TestManager, (), (), ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(TestManager {
            list_modal_open: false,
            editor_modal_open: false,
        }),
        (),
        false,
    );

    app.open_quit_confirm();

    let plan = app.handle_quit_confirm_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::NONE,
    ));

    assert_eq!(plan, Some(crate::tui::run::QuitConfirmPlan::Dismiss));
    assert!(!app.quit_confirm_open());
}

#[test]
fn console_manager_stage_routes_by_variant() {
    assert_eq!(
        ConsoleManagerStage::<(), (), ()>::List.route(),
        ConsoleManagerStageRoute::List
    );
    assert_eq!(
        ConsoleManagerStage::<(), (), ()>::Editor(()).route(),
        ConsoleManagerStageRoute::Editor
    );
    assert_eq!(
        ConsoleManagerStage::<(), (), ()>::Settings(()).route(),
        ConsoleManagerStageRoute::Settings
    );
    assert_eq!(
        ConsoleManagerStage::<(), (), ()>::CreatePrelude(()).route(),
        ConsoleManagerStageRoute::CreatePrelude
    );
    assert_eq!(
        ConsoleManagerStage::<(), (), ()>::ConfirmDelete {
            name: "workspace".to_owned(),
            state: crate::tui::components::ConfirmState::new("Delete?"),
        }
        .route(),
        ConsoleManagerStageRoute::ConfirmDelete
    );
    assert_eq!(
        ConsoleManagerStage::<(), (), ()>::ConfirmInstancePurge {
            container: "container".to_owned(),
            label: "label".to_owned(),
            state: crate::tui::components::ConfirmState::new("Purge?"),
        }
        .route(),
        ConsoleManagerStageRoute::ConfirmInstancePurge
    );
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

#[test]
fn apply_manager_stage_updates_storage() {
    let mut state = TestStageState::default();

    apply_manager_stage(&mut state, ConsoleManagerStage::List);

    assert_eq!(
        state.stage.as_ref().map(ConsoleManagerStage::route),
        Some(ConsoleManagerStageRoute::List)
    );
}

#[test]
fn console_manager_stage_reports_modal_facts() {
    type Stage = ConsoleManagerStage<ConsoleCreatePreludeState<()>, TestEditor, TestSettings>;

    assert_eq!(Stage::List.modal_facts(), ConsoleStageModalFacts::default());
    assert_eq!(
        Stage::Editor(TestEditor {
            modal_open: true,
            footer_height: 4,
        })
        .modal_facts(),
        ConsoleStageModalFacts {
            editor_modal_open: true,
            ..ConsoleStageModalFacts::default()
        }
    );

    let settings_facts = ConsoleStageModalFacts {
        settings_error_popup_open: true,
        settings_auth_modal_open: true,
        ..ConsoleStageModalFacts::default()
    };
    assert_eq!(
        Stage::Settings(TestSettings {
            facts: settings_facts,
            footer_height: 6,
        })
        .modal_facts(),
        settings_facts
    );

    assert_eq!(
        Stage::CreatePrelude(ConsoleCreatePreludeState {
            step: CreateStep::PickFirstMountSrc,
            pending_mount_src: None,
            pending_mount_dst: None,
            pending_readonly: false,
            pending_workdir: None,
            pending_name: None,
            modal: Some(()),
            last_browser_cwd: None,
            used_edit_dst: false,
        })
        .modal_facts(),
        ConsoleStageModalFacts {
            create_prelude_modal_open: true,
            ..ConsoleStageModalFacts::default()
        }
    );

    assert_eq!(
        Stage::ConfirmDelete {
            name: "workspace".to_owned(),
            state: crate::tui::components::ConfirmState::new("Delete?"),
        }
        .modal_facts(),
        ConsoleStageModalFacts {
            destructive_confirm_open: true,
            ..ConsoleStageModalFacts::default()
        }
    );
}

#[test]
fn console_manager_stage_reports_footer_height_facts() {
    type Stage = ConsoleManagerStage<(), TestEditor, TestSettings>;

    assert_eq!(
        Stage::Editor(TestEditor {
            modal_open: false,
            footer_height: 4,
        })
        .footer_height_facts(2),
        crate::tui::view::StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Editor,
            workspace_footer_height: 2,
            editor_footer_height: 4,
            settings_footer_height: 0,
        }
    );
    assert_eq!(
        Stage::Settings(TestSettings {
            facts: ConsoleStageModalFacts::default(),
            footer_height: 6,
        })
        .footer_height_facts(2),
        crate::tui::view::StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Settings,
            workspace_footer_height: 2,
            editor_footer_height: 0,
            settings_footer_height: 6,
        }
    );
    assert_eq!(
        Stage::List.footer_height_facts(2),
        crate::tui::view::StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::List,
            workspace_footer_height: 2,
            editor_footer_height: 0,
            settings_footer_height: 0,
        }
    );
}

#[test]
fn console_manager_stage_takes_pending_token_generate_from_editor_or_settings() {
    type Stage = ConsoleManagerStage<(), TestTokenDrain, TestTokenDrain>;

    let mut editor = Stage::Editor(TestTokenDrain { pending: Some(7) });
    assert_eq!(editor.take_pending_token_generate(), Some(7));
    assert_eq!(editor.take_pending_token_generate(), None);

    let mut settings = Stage::Settings(TestTokenDrain { pending: Some(9) });
    assert_eq!(settings.take_pending_token_generate(), Some(9));
    assert_eq!(settings.take_pending_token_generate(), None);

    let mut list = Stage::List;
    assert_eq!(list.take_pending_token_generate(), None);

    let mut create = Stage::CreatePrelude(());
    assert_eq!(create.take_pending_token_generate(), None);

    let mut delete = Stage::ConfirmDelete {
        name: "workspace".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete?"),
    };
    assert_eq!(delete.take_pending_token_generate(), None);

    let mut purge = Stage::ConfirmInstancePurge {
        container: "container".to_owned(),
        label: "label".to_owned(),
        state: crate::tui::components::ConfirmState::new("Purge?"),
    };
    assert_eq!(purge.take_pending_token_generate(), None);
}

#[test]
fn console_manager_stage_polls_pending_role_load_from_editor_only() {
    type Stage = ConsoleManagerStage<(), TestRoleLoad, ()>;

    let mut editor = Stage::Editor(TestRoleLoad { pending: Some(3) });
    let Some((load, result)) = editor.poll_pending_role_load() else {
        panic!("expected pending role load");
    };
    assert_eq!(load, 3);
    result.unwrap();
    assert!(editor.poll_pending_role_load().is_none());

    assert!(Stage::List.poll_pending_role_load().is_none());
    assert!(Stage::Settings(()).poll_pending_role_load().is_none());
    assert!(Stage::CreatePrelude(()).poll_pending_role_load().is_none());
    assert!(
        Stage::ConfirmDelete {
            name: "workspace".to_owned(),
            state: crate::tui::components::ConfirmState::new("Delete?"),
        }
        .poll_pending_role_load()
        .is_none()
    );
    assert!(
        Stage::ConfirmInstancePurge {
            container: "container".to_owned(),
            label: "label".to_owned(),
            state: crate::tui::components::ConfirmState::new("Purge?"),
        }
        .poll_pending_role_load()
        .is_none()
    );
}

#[test]
fn console_manager_stage_polls_pending_drift_check_from_editor_only() {
    type Stage = ConsoleManagerStage<(), TestDriftCheck, ()>;

    let mut editor = Stage::Editor(TestDriftCheck {
        pending: Some((3, "drift")),
    });
    let Some((check, result)) = editor.poll_pending_drift_check() else {
        panic!("expected pending drift check");
    };
    assert_eq!(check, 3);
    assert_eq!(result.ok(), Some("drift"));
    assert!(editor.poll_pending_drift_check().is_none());

    assert!(Stage::List.poll_pending_drift_check().is_none());
    assert!(Stage::Settings(()).poll_pending_drift_check().is_none());
    assert!(
        Stage::CreatePrelude(())
            .poll_pending_drift_check()
            .is_none()
    );
    assert!(
        Stage::ConfirmDelete {
            name: "workspace".to_owned(),
            state: crate::tui::components::ConfirmState::new("Delete?"),
        }
        .poll_pending_drift_check()
        .is_none()
    );
    assert!(
        Stage::ConfirmInstancePurge {
            container: "container".to_owned(),
            label: "label".to_owned(),
            state: crate::tui::components::ConfirmState::new("Purge?"),
        }
        .poll_pending_drift_check()
        .is_none()
    );
}

#[test]
fn console_manager_stage_polls_pending_isolation_cleanup_from_editor_only() {
    type Stage = ConsoleManagerStage<(), TestIsolationCleanup, ()>;

    let mut editor = Stage::Editor(TestIsolationCleanup { pending: Some(5) });
    let Some((cleanup, result)) = editor.poll_pending_isolation_cleanup() else {
        panic!("expected pending isolation cleanup");
    };
    assert_eq!(cleanup, 5);
    result.unwrap();
    assert!(editor.poll_pending_isolation_cleanup().is_none());

    assert!(Stage::List.poll_pending_isolation_cleanup().is_none());
    assert!(
        Stage::Settings(())
            .poll_pending_isolation_cleanup()
            .is_none()
    );
    assert!(
        Stage::CreatePrelude(())
            .poll_pending_isolation_cleanup()
            .is_none()
    );
    assert!(
        Stage::ConfirmDelete {
            name: "workspace".to_owned(),
            state: crate::tui::components::ConfirmState::new("Delete?"),
        }
        .poll_pending_isolation_cleanup()
        .is_none()
    );
    assert!(
        Stage::ConfirmInstancePurge {
            container: "container".to_owned(),
            label: "label".to_owned(),
            state: crate::tui::components::ConfirmState::new("Purge?"),
        }
        .poll_pending_isolation_cleanup()
        .is_none()
    );
}

#[test]
fn console_manager_stage_polls_pending_op_commit_with_origin() {
    type Stage = ConsoleManagerStage<(), TestOpCommit, TestOpCommit>;

    let mut editor = Stage::Editor(TestOpCommit {
        pending: Some((3, Ok(()))),
    });
    let Some(resolution) = editor.poll_pending_op_commit() else {
        panic!("expected pending editor op commit");
    };
    assert_eq!(resolution.op_ref, 3);
    resolution.result.unwrap();
    assert_eq!(
        resolution.origin,
        super::ConsolePendingOpCommitOrigin::Editor
    );
    assert!(editor.poll_pending_op_commit().is_none());

    let mut settings = Stage::Settings(TestOpCommit {
        pending: Some((5, Ok(()))),
    });
    let Some(resolution) = settings.poll_pending_op_commit() else {
        panic!("expected pending settings op commit");
    };
    assert_eq!(resolution.op_ref, 5);
    resolution.result.unwrap();
    assert_eq!(
        resolution.origin,
        super::ConsolePendingOpCommitOrigin::Settings
    );
    assert!(settings.poll_pending_op_commit().is_none());

    assert!(Stage::List.poll_pending_op_commit().is_none());
    assert!(Stage::CreatePrelude(()).poll_pending_op_commit().is_none());
    assert!(
        Stage::ConfirmDelete {
            name: "workspace".to_owned(),
            state: crate::tui::components::ConfirmState::new("Delete?"),
        }
        .poll_pending_op_commit()
        .is_none()
    );
    assert!(
        Stage::ConfirmInstancePurge {
            container: "container".to_owned(),
            label: "label".to_owned(),
            state: crate::tui::components::ConfirmState::new("Purge?"),
        }
        .poll_pending_op_commit()
        .is_none()
    );
}

#[test]
fn console_manager_stage_reports_debug_stage() {
    type Stage =
        ConsoleManagerStage<ConsoleCreatePreludeState<TestDebugModal>, TestEditor, TestSettings>;

    assert_eq!(Stage::List.debug_stage(), ConsoleStageDebug::List);
    assert_eq!(
        Stage::Editor(TestEditor {
            modal_open: true,
            footer_height: 4,
        })
        .debug_stage(),
        ConsoleStageDebug::Editor {
            mode: "TestMode".to_owned(),
            tab: "TestTab".to_owned(),
            field: "TestField".to_owned(),
            modal: Some(ModalDebugKind::TextInput),
        }
    );
    assert_eq!(
        Stage::CreatePrelude(ConsoleCreatePreludeState {
            step: CreateStep::PickFirstMountSrc,
            pending_mount_src: None,
            pending_mount_dst: None,
            pending_readonly: false,
            pending_workdir: None,
            pending_name: None,
            modal: Some(TestDebugModal),
            last_browser_cwd: None,
            used_edit_dst: false,
        })
        .debug_stage(),
        ConsoleStageDebug::CreatePrelude {
            step: "PickFirstMountSrc".to_owned(),
            modal: Some(ModalDebugKind::ErrorPopup),
        }
    );
    assert_eq!(
        Stage::Settings(TestSettings {
            facts: ConsoleStageModalFacts::default(),
            footer_height: 6,
        })
        .debug_stage(),
        ConsoleStageDebug::Settings {
            tab: "Mounts".to_owned(),
            selected: 2,
            modal: None,
        }
    );
}

#[test]
fn console_input_dispatch_plan_routes_modal_precedence_before_stage() {
    let base = ConsoleInputDispatchFacts {
        list_modal_open: false,
        inline_new_session_picker_open: false,
        inline_provider_picker_open: false,
        launch_provider_picker_open: false,
        inline_agent_picker_open: false,
        inline_role_picker_open: false,
        editor_modal_open: false,
        settings_error_popup_open: false,
        settings_mounts_modal_open: false,
        settings_env_modal_open: false,
        settings_auth_modal_open: false,
        create_prelude_modal_open: false,
        stage_route: ConsoleManagerStageRoute::Settings,
    };

    assert_eq!(
        console_input_dispatch_plan(base),
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::Settings)
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            list_modal_open: true,
            editor_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::ListModal
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            inline_new_session_picker_open: true,
            inline_role_picker_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::InlineNewSessionPicker
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            inline_provider_picker_open: true,
            launch_provider_picker_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::InlineProviderPicker
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            launch_provider_picker_open: true,
            inline_agent_picker_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::LaunchProviderPicker
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            inline_agent_picker_open: true,
            inline_role_picker_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::InlineAgentPicker
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            inline_role_picker_open: true,
            editor_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::InlineRolePicker
    );
}

#[test]
fn console_input_dispatch_plan_routes_stage_modal_precedence() {
    let base = ConsoleInputDispatchFacts {
        list_modal_open: false,
        inline_new_session_picker_open: false,
        inline_provider_picker_open: false,
        launch_provider_picker_open: false,
        inline_agent_picker_open: false,
        inline_role_picker_open: false,
        editor_modal_open: false,
        settings_error_popup_open: false,
        settings_mounts_modal_open: false,
        settings_env_modal_open: false,
        settings_auth_modal_open: false,
        create_prelude_modal_open: false,
        stage_route: ConsoleManagerStageRoute::CreatePrelude,
    };

    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            editor_modal_open: true,
            settings_error_popup_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::EditorModal
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            settings_error_popup_open: true,
            settings_mounts_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::SettingsErrorPopup
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            settings_mounts_modal_open: true,
            settings_env_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::SettingsMountsModal
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            settings_env_modal_open: true,
            settings_auth_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::SettingsEnvDialog
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            settings_auth_modal_open: true,
            create_prelude_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::SettingsAuthDialog
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            create_prelude_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::CreatePreludeModal
    );
}

#[test]
fn create_prelude_completion_status_routes_modal_complete_and_cancel() {
    assert_eq!(
        create_prelude_completion_status(true, true),
        CreatePreludeCompletionStatus::InProgress
    );
    assert_eq!(
        create_prelude_completion_status(false, true),
        CreatePreludeCompletionStatus::Complete
    );
    assert_eq!(
        create_prelude_completion_status(false, false),
        CreatePreludeCompletionStatus::Cancelled
    );
}

#[test]
fn create_prelude_key_plan_routes_escape_to_list() {
    assert_eq!(
        create_prelude_key_plan(crossterm::event::KeyCode::Esc),
        CreatePreludeKeyPlan::ReturnToList
    );
    assert_eq!(
        create_prelude_key_plan(crossterm::event::KeyCode::Enter),
        CreatePreludeKeyPlan::Continue
    );
}

#[test]
fn create_prelude_modal_step_routes_modal_facts_by_precedence() {
    assert_eq!(
        create_prelude_modal_step(true, true, true, true, true),
        CreatePreludeModalStep::FileBrowserSrc
    );
    assert_eq!(
        create_prelude_modal_step(false, true, true, true, true),
        CreatePreludeModalStep::MountDstChoice
    );
    assert_eq!(
        create_prelude_modal_step(false, false, true, true, true),
        CreatePreludeModalStep::TextInputDst
    );
    assert_eq!(
        create_prelude_modal_step(false, false, false, true, true),
        CreatePreludeModalStep::WorkdirPick
    );
    assert_eq!(
        create_prelude_modal_step(false, false, false, false, true),
        CreatePreludeModalStep::TextInputName
    );
    assert_eq!(
        create_prelude_modal_step(false, false, false, false, false),
        CreatePreludeModalStep::Other
    );
}

#[test]
fn create_prelude_workdir_cancel_plan_reopens_prior_dst_step() {
    assert_eq!(
        create_prelude_workdir_cancel_plan(true),
        CreatePreludeWorkdirCancelPlan::ReopenTextInputDst
    );
    assert_eq!(
        create_prelude_workdir_cancel_plan(false),
        CreatePreludeWorkdirCancelPlan::ReopenMountDstChoice
    );
}

#[test]
fn create_prelude_file_browser_plan_routes_browser_outcomes() {
    use crate::tui::components::file_browser::FileBrowserOutcome;

    let path = PathBuf::from("/tmp/workspace");
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::Cancel),
        CreatePreludeFileBrowserPlan::CancelPrelude
    );
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::ResolveGitUrl(
            path.clone()
        )),
        CreatePreludeFileBrowserPlan::ResolveGitUrl(path.clone())
    );
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::OpenGitUrl(
            "file:///tmp/workspace".to_owned()
        )),
        CreatePreludeFileBrowserPlan::OpenUrl("file:///tmp/workspace".to_owned())
    );
    assert_eq!(
        create_prelude_file_browser_plan::<PathBuf>(FileBrowserOutcome::Continue),
        CreatePreludeFileBrowserPlan::Continue
    );
    assert_eq!(
        create_prelude_file_browser_plan(FileBrowserOutcome::<PathBuf>::NavigateTo(path.clone())),
        CreatePreludeFileBrowserPlan::ApplyFileBrowserOutcome(FileBrowserOutcome::NavigateTo(path))
    );
}

#[test]
fn create_prelude_mount_dst_choice_plan_routes_choice_outcomes() {
    use crate::tui::components::mount_dst_choice::MountDstChoice;

    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(
            MountDstChoice::SamePath
        )),
        CreatePreludeMountDstChoicePlan::CommitSamePath
    );
    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Commit(
            MountDstChoice::Edit
        )),
        CreatePreludeMountDstChoicePlan::OpenEditInput
    );
    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Cancel),
        CreatePreludeMountDstChoicePlan::ReopenFileBrowserAtLastCwd
    );
    assert_eq!(
        create_prelude_mount_dst_choice_plan(jackin_tui::ModalOutcome::Continue),
        CreatePreludeMountDstChoicePlan::Continue
    );
}

#[test]
fn create_prelude_text_input_dst_plan_routes_input_outcomes() {
    assert_eq!(
        create_prelude_text_input_dst_plan(jackin_tui::ModalOutcome::Commit(
            "/workspace".to_owned()
        )),
        CreatePreludeTextInputDstPlan::Commit("/workspace".to_owned())
    );
    assert_eq!(
        create_prelude_text_input_dst_plan::<String>(jackin_tui::ModalOutcome::Cancel),
        CreatePreludeTextInputDstPlan::ReopenMountDstChoice
    );
    assert_eq!(
        create_prelude_text_input_dst_plan::<String>(jackin_tui::ModalOutcome::Continue),
        CreatePreludeTextInputDstPlan::Continue
    );
}

#[test]
fn create_prelude_text_input_name_plan_routes_input_outcomes() {
    assert_eq!(
        create_prelude_text_input_name_plan(jackin_tui::ModalOutcome::Commit(
            "workspace".to_owned()
        )),
        CreatePreludeTextInputNamePlan::Commit("workspace".to_owned())
    );
    assert_eq!(
        create_prelude_text_input_name_plan::<String>(jackin_tui::ModalOutcome::Cancel),
        CreatePreludeTextInputNamePlan::ReopenWorkdirPick
    );
    assert_eq!(
        create_prelude_text_input_name_plan::<String>(jackin_tui::ModalOutcome::Continue),
        CreatePreludeTextInputNamePlan::Continue
    );
}

#[test]
fn create_prelude_workdir_pick_plan_routes_input_outcomes() {
    assert_eq!(
        create_prelude_workdir_pick_plan(jackin_tui::ModalOutcome::Commit("src".to_owned()), true),
        CreatePreludeWorkdirPickPlan::Commit("src".to_owned())
    );
    assert_eq!(
        create_prelude_workdir_pick_plan::<String>(jackin_tui::ModalOutcome::Cancel, true),
        CreatePreludeWorkdirPickPlan::ReopenTextInputDst
    );
    assert_eq!(
        create_prelude_workdir_pick_plan::<String>(jackin_tui::ModalOutcome::Cancel, false),
        CreatePreludeWorkdirPickPlan::ReopenMountDstChoice
    );
    assert_eq!(
        create_prelude_workdir_pick_plan::<String>(jackin_tui::ModalOutcome::Continue, true),
        CreatePreludeWorkdirPickPlan::Continue
    );
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

type PreludeStepTestModal = ConsoleModal<
    crate::tui::screens::editor::model::TextInputTarget,
    (),
    crate::tui::screens::editor::model::FileBrowserTarget,
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

#[test]
fn console_modal_create_prelude_step_maps_create_modal_targets() {
    use crate::tui::screens::editor::model::{FileBrowserTarget, TextInputTarget};

    assert_eq!(
        PreludeStepTestModal::FileBrowser {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: TestFileBrowser,
        }
        .create_prelude_step(),
        CreatePreludeModalStep::FileBrowserSrc
    );
    assert_eq!(
        PreludeStepTestModal::MountDstChoice {
            target: FileBrowserTarget::CreateFirstMountSrc,
            state: (),
        }
        .create_prelude_step(),
        CreatePreludeModalStep::MountDstChoice
    );
    assert_eq!(
        PreludeStepTestModal::TextInput {
            target: TextInputTarget::MountDst,
            state: (),
        }
        .create_prelude_step(),
        CreatePreludeModalStep::TextInputDst
    );
    assert_eq!(
        PreludeStepTestModal::WorkdirPick { state: () }.create_prelude_step(),
        CreatePreludeModalStep::WorkdirPick
    );
    assert_eq!(
        PreludeStepTestModal::TextInput {
            target: TextInputTarget::Name,
            state: (),
        }
        .create_prelude_step(),
        CreatePreludeModalStep::TextInputName
    );
    assert_eq!(
        PreludeStepTestModal::FileBrowser {
            target: FileBrowserTarget::EditAddMountSrc,
            state: TestFileBrowser,
        }
        .create_prelude_step(),
        CreatePreludeModalStep::Other
    );
}

#[test]
fn console_modal_letter_input_kind_maps_text_filters_and_other_modals() {
    assert_eq!(
        RectTestModal::TextInput {
            target: (),
            state: (),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::TextInput)
    );
    assert_eq!(
        RectTestModal::RolePicker {
            state: TestRolePicker(2),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::FilterPicker)
    );
    assert_eq!(
        RectTestModal::RoleOverridePicker {
            state: TestRolePicker(2),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::FilterPicker)
    );
    assert_eq!(
        RectTestModal::OpPicker {
            secrets_target: None,
            state: Box::new(TestOpPicker(false)),
        }
        .letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::FilterPicker)
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.letter_input_kind(),
        Some(crate::tui::run::LetterInputModalKind::Other)
    );
}

#[test]
fn console_modal_list_key_target_maps_list_modal_key_handlers() {
    assert_eq!(
        RectTestModal::GithubPicker {
            state: TestGithubPicker(2)
        }
        .list_key_target(),
        crate::tui::update::ListModalKeyTarget::GithubPicker
    );
    assert_eq!(
        RectTestModal::RolePicker {
            state: TestRolePicker(2)
        }
        .list_key_target(),
        crate::tui::update::ListModalKeyTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.list_key_target(),
        crate::tui::update::ListModalKeyTarget::ErrorPopup
    );
    assert_eq!(
        RectTestModal::ContainerInfo {
            state: TestContainerInfo
        }
        .list_key_target(),
        crate::tui::update::ListModalKeyTarget::ContainerInfo
    );
    assert_eq!(
        RectTestModal::StatusPopup { state: () }.list_key_target(),
        crate::tui::update::ListModalKeyTarget::Dismiss
    );
}

#[test]
fn console_modal_list_scroll_target_maps_scrollable_list_modals() {
    assert_eq!(
        RectTestModal::GithubPicker {
            state: TestGithubPicker(2)
        }
        .list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::GithubPicker
    );
    assert_eq!(
        RectTestModal::RolePicker {
            state: TestRolePicker(2)
        }
        .list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::OpPicker {
            secrets_target: None,
            state: Box::new(TestOpPicker(false))
        }
        .list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::OpPicker
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.list_scroll_target(),
        crate::tui::update::ListModalScrollTarget::None
    );
}

#[test]
fn console_modal_shared_scroll_target_maps_reused_picker_modals() {
    assert_eq!(
        RectTestModal::WorkdirPick { state: () }.shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::WorkdirPick
    );
    assert_eq!(
        RectTestModal::RoleOverridePicker {
            state: TestRolePicker(2)
        }
        .shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::AuthRolePicker {
            state: TestRolePicker(2)
        }
        .shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::RolePicker
    );
    assert_eq!(
        RectTestModal::OpPicker {
            secrets_target: None,
            state: Box::new(TestOpPicker(false))
        }
        .shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::OpPicker
    );
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.shared_scroll_target(),
        crate::tui::update::SharedModalScrollTarget::None
    );
}

#[test]
fn console_modal_ticks_op_picker_animation_only() {
    let mut op_picker = RectTestModal::OpPicker {
        secrets_target: None,
        state: Box::new(TestOpPicker(true)),
    };
    assert!(op_picker.tick_active_animation());

    let mut idle_op_picker = RectTestModal::OpPicker {
        secrets_target: None,
        state: Box::new(TestOpPicker(false)),
    };
    assert!(!idle_op_picker.tick_active_animation());

    let mut error = RectTestModal::ErrorPopup { state: TestError };
    assert!(!error.tick_active_animation());
}

struct TestAnimationTick(bool);

impl ConsoleAnimationTick for TestAnimationTick {
    fn tick_active_animation(&mut self) -> bool {
        self.0
    }
}

#[test]
fn console_manager_stage_ticks_editor_and_settings_only() {
    type Stage = ConsoleManagerStage<(), TestAnimationTick, TestAnimationTick>;

    let mut editor = Stage::Editor(TestAnimationTick(true));
    assert!(editor.tick_active_animation());

    let mut settings = Stage::Settings(TestAnimationTick(true));
    assert!(settings.tick_active_animation());

    let mut idle_editor = Stage::Editor(TestAnimationTick(false));
    assert!(!idle_editor.tick_active_animation());

    let mut list = Stage::List;
    assert!(!list.tick_active_animation());

    let mut create = Stage::CreatePrelude(());
    assert!(!create.tick_active_animation());

    let mut delete = Stage::ConfirmDelete {
        name: "workspace".to_owned(),
        state: crate::tui::components::ConfirmState::new("Delete?"),
    };
    assert!(!delete.tick_active_animation());
}

#[test]
fn create_prelude_completed_requires_name_and_mount_fields() {
    let mut prelude = ConsoleCreatePreludeState::<()>::new();
    prelude.accept_mount_src(PathBuf::from("/host/proj"));
    prelude.accept_mount_dst("/work/proj".into(), true);
    prelude.accept_workdir("/work/proj".into());

    assert!(prelude.completed().is_none());

    prelude.accept_name("proj".into());
    let (name, workspace) = prelude.completed().expect("complete prelude");

    assert_eq!(name, "proj");
    assert_eq!(workspace.workdir, "/work/proj");
    assert_eq!(workspace.mounts.len(), 1);
    assert_eq!(workspace.mounts[0].src, "/host/proj");
    assert_eq!(workspace.mounts[0].dst, "/work/proj");
    assert!(workspace.mounts[0].readonly);
    assert_eq!(workspace.mounts[0].isolation, MountIsolation::Shared);
}

#[test]
fn create_prelude_builds_pending_first_mount() {
    let mut prelude = ConsoleCreatePreludeState::<()>::new();
    assert!(prelude.pending_first_mount().is_none());

    prelude.accept_mount_src(PathBuf::from("/host/proj"));
    prelude.accept_mount_dst("/work/proj".into(), true);
    let mount = prelude
        .pending_first_mount()
        .expect("src and dst should build mount");

    assert_eq!(mount.src, "/host/proj");
    assert_eq!(mount.dst, "/work/proj");
    assert!(mount.readonly);
    assert_eq!(mount.isolation, MountIsolation::Shared);
}

#[test]
fn create_prelude_opens_workdir_pick_from_pending_mount() {
    let mut prelude = ConsoleCreatePreludeState::<jackin_config::MountConfig>::new();
    assert!(!prelude.open_workdir_pick_from_pending_mount(|mount| mount));
    assert!(prelude.modal.is_none());

    prelude.accept_mount_src(PathBuf::from("/host/proj"));
    prelude.accept_mount_dst("/work/proj".into(), false);

    assert!(prelude.open_workdir_pick_from_pending_mount(|mount| mount));

    let Some(mount) = prelude.modal else {
        panic!("expected workdir pick modal payload");
    };
    assert_eq!(mount.src, "/host/proj");
    assert_eq!(mount.dst, "/work/proj");
    assert!(!mount.readonly);
}

#[test]
fn create_prelude_reopens_mount_dst_choice_from_source() {
    let mut prelude = ConsoleCreatePreludeState::<String>::new();
    prelude.accept_mount_src(PathBuf::from("/host/proj"));

    prelude.reopen_mount_dst_choice(|src| src);

    assert_eq!(prelude.modal.as_deref(), Some("/host/proj"));
}

#[test]
fn console_modal_reports_debug_kind() {
    type TestModal = ConsoleModal<
        &'static str,
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
    >;

    let modal = TestModal::TextInput {
        target: "name",
        state: (),
    };

    assert_eq!(modal.debug_kind(), ModalDebugKind::TextInput);
}

#[test]
fn console_modal_reports_auth_form_generate_eligibility() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
    form.set_mode(crate::tui::auth::AuthMode::OAuthToken);
    let modal = TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    };

    assert!(modal.auth_form_can_generate_token(true));
    assert!(!modal.auth_form_can_generate_token(false));
}

#[test]
fn console_modal_opens_auth_generate_source_picker() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        &'static str,
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = Some(TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    });
    let mut parents = Vec::new();

    let target =
        crate::tui::auth_config::ModalAuthTokenGenerateStart::open_auth_generate_source_picker(
            &mut modal,
            &mut parents,
            "source-picker",
        )
        .expect("open auth form should move to source picker");

    assert!(matches!(
        target,
        crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude
        }
    ));
    assert_eq!(parents.len(), 1);
    assert!(matches!(modal, Some(TestModal::AuthSourcePicker { .. })));
}

#[test]
fn console_modal_opens_auth_source_picker_from_form() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        &'static str,
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
    form.set_mode(crate::tui::auth::AuthMode::ApiKey);
    let mut modal = Some(TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: "existing".into(),
    });
    let mut parents = Vec::new();

    let opened = crate::tui::auth_config::ModalAuthSourcePickerOpen::open_auth_source_picker(
        &mut modal,
        &mut parents,
        |env_var| env_var,
    );

    assert!(opened);
    assert_eq!(parents.len(), 1);
    let expected_env_var = crate::tui::auth::AuthKind::Claude
        .required_env_var(crate::tui::auth::AuthMode::ApiKey)
        .expect("Claude API key mode requires env var");
    assert!(matches!(
        modal,
        Some(TestModal::AuthSourcePicker { state }) if state == expected_env_var
    ));
}

#[test]
fn console_modal_opens_auth_source_folder_browser() {
    type TestModal = ConsoleModal<
        (),
        (),
        &'static str,
        &'static str,
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude)
            .with_source_folder(
                None,
                Some(
                    crate::tui::components::editor_rows::AuthSourceFolderDisplay {
                        kind: crate::tui::components::editor_rows::AuthSourceFolderKind::Default,
                        path: "~/.claude".into(),
                    },
                ),
            );
    form.set_mode(crate::tui::auth::AuthMode::Sync);
    let mut modal = Some(TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: crate::tui::screens::settings::model::AuthFormFocus::SourceFolder,
        literal_buffer: String::new(),
    });
    let mut parents = Vec::new();

    let opened =
        crate::tui::auth_config::ModalAuthSourceFolderBrowserOpen::open_auth_source_folder_browser(
            &mut modal,
            &mut parents,
            crate::tui::screens::settings::model::AuthFormFocus::SourceFolder,
            "auth-source-folder",
            || Ok::<_, ()>("browser"),
        );

    assert_eq!(
        opened,
        crate::tui::auth_config::AuthSourceFolderBrowserOpenResult::Opened
    );
    assert_eq!(parents.len(), 1);
    assert!(matches!(
        modal,
        Some(TestModal::FileBrowser {
            target: "auth-source-folder",
            state: "browser"
        })
    ));
}

#[test]
fn console_modal_opens_plain_source_text_input() {
    type TestModal = ConsoleModal<
        &'static str,
        String,
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
        literal_buffer: "existing".into(),
    }];

    let opened =
        crate::tui::auth_config::ModalAuthPlainSourceOpen::open_auth_plain_source_text_input(
            &mut modal,
            &mut parents,
            crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            "auth",
            |literal| literal,
        );

    assert!(opened);
    assert_eq!(parents.len(), 1);
    assert!(
        matches!(modal, Some(TestModal::TextInput { target: "auth", state }) if state == "existing")
    );
}

#[test]
fn console_modal_opens_auth_op_picker() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        &'static str,
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::Mode,
        literal_buffer: String::new(),
    }];

    let opened = crate::tui::auth_config::ModalAuthOpPickerOpen::open_auth_op_picker(
        &mut modal,
        &mut parents,
        crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        || "op-picker",
    );

    assert!(opened);
    assert!(matches!(
        parents.last(),
        Some(TestModal::AuthForm {
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            ..
        })
    ));
    assert!(matches!(modal, Some(TestModal::OpPicker { state, .. }) if *state == "op-picker"));
}

#[test]
fn console_modal_applies_auth_plain_text() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: String::new(),
    }];

    let applied = crate::tui::auth_config::ModalAuthFormCredentialApply::apply_auth_plain_text(
        &mut modal,
        &mut parents,
        crate::tui::screens::settings::model::AuthFormFocus::Save,
        "token",
    );

    assert!(applied);
    assert!(parents.is_empty());
    assert!(matches!(
        modal,
        Some(TestModal::AuthForm {
            state,
            focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
            literal_buffer,
            ..
        }) if state.literal_buffer() == "token" && literal_buffer == "token"
    ));
}

#[test]
fn console_modal_restores_auth_form_modal() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(crate::tui::components::auth_panel::AuthForm::new(
            crate::tui::auth::AuthKind::Claude,
        )),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: "existing".into(),
    }];

    let restored = crate::tui::auth_config::ModalAuthFormCredentialApply::restore_auth_form_modal(
        &mut modal,
        &mut parents,
    );

    assert!(restored);
    assert!(parents.is_empty());
    assert!(matches!(
        modal,
        Some(TestModal::AuthForm {
            focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
            literal_buffer,
            ..
        }) if literal_buffer == "existing"
    ));
}

#[test]
fn console_modal_applies_auth_op_ref() {
    type TestModal = ConsoleModal<
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        (),
        crate::tui::screens::settings::model::AuthFormTarget<crate::tui::auth::AuthKind>,
        crate::tui::components::auth_panel::AuthForm<jackin_core::EnvValue>,
        crate::tui::screens::settings::model::AuthFormFocus,
        (),
    >;

    let mut form =
        crate::tui::components::auth_panel::AuthForm::new(crate::tui::auth::AuthKind::Claude);
    form.set_mode(crate::tui::auth::AuthMode::ApiKey);
    let op_ref = jackin_core::OpRef {
        op: "op://vault/item/field".into(),
        path: "Vault/Item/Field".into(),
        account: None,
        on_demand: false,
    };
    let mut modal = None;
    let mut parents = vec![TestModal::AuthForm {
        target: crate::tui::screens::settings::model::AuthFormTarget::Workspace {
            kind: crate::tui::auth::AuthKind::Claude,
        },
        state: Box::new(form),
        focus: crate::tui::screens::settings::model::AuthFormFocus::CredentialSource,
        literal_buffer: String::new(),
    }];

    let applied = crate::tui::auth_config::ModalAuthFormOpRefApply::apply_auth_op_ref(
        &mut modal,
        &mut parents,
        crate::tui::screens::settings::model::AuthFormFocus::Save,
        op_ref.clone(),
    );

    assert!(applied);
    assert!(parents.is_empty());
    assert!(matches!(
        modal,
        Some(TestModal::AuthForm {
            state,
            focus: crate::tui::screens::settings::model::AuthFormFocus::Save,
            ..
        }) if matches!(
            &state.credential,
            crate::tui::components::auth_panel::CredentialInput::OpRef(value)
                if *value == op_ref
        )
    ));
}

#[test]
fn console_modal_reports_rect_mode() {
    let modal = RectTestModal::RolePicker {
        state: TestRolePicker(5),
    };

    assert_eq!(
        modal.rect_mode(Rect::new(0, 0, 100, 40)),
        ModalRectMode::RolePicker { filtered_len: 5 }
    );
}

#[test]
fn console_modal_error_rect_mode_uses_required_height() {
    let modal = RectTestModal::ErrorPopup { state: TestError };

    assert_eq!(
        modal.rect_mode(Rect::new(0, 0, 100, 40)),
        ModalRectMode::ErrorPopup {
            required_height: 14
        }
    );
}

#[test]
fn console_modal_container_info_rect_reports_only_container_info_area() {
    let outer = Rect::new(0, 0, 100, 40);
    let modal = RectTestModal::ContainerInfo {
        state: TestContainerInfo,
    };

    assert_eq!(modal.container_info_rect(outer), Some(modal.rect(outer)));
    assert_eq!(
        RectTestModal::ErrorPopup { state: TestError }.container_info_rect(outer),
        None
    );
}

#[test]
fn console_modal_reports_footer_items() {
    let modal = RectTestModal::RolePicker {
        state: TestRolePicker(5),
    };

    assert!(
        modal
            .footer_items(false)
            .contains(&termrock::widgets::HintSpan::Text("filter"))
    );
}

#[test]
fn console_modal_footer_items_for_area_reflects_container_info_overflow() {
    let modal = RectTestModal::ContainerInfo {
        state: TestContainerInfo,
    };

    assert!(
        modal
            .footer_items_for_area(false, Rect::new(0, 0, 100, 20))
            .contains(&termrock::widgets::HintSpan::Text("scroll"))
    );
}
