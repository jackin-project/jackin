// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::tui::components::ConfirmState;
use crate::tui::components::role_picker::RolePickerState;
use crate::tui::debug::{
    ConsoleCreatePreludeDebugFacts, ConsoleEditorDebugFacts, ConsoleSettingsDebugFacts,
    ConsoleStageDebug,
};
use crate::tui::screens::workflows::model::WorkflowScreen;
use crate::tui::state::{ManagerStage, ManagerState};
use crate::tui::update::ListModalState;
use crate::tui::view::StageFooterHeightFacts;

fn test_role() -> jackin_core::RoleSelector {
    jackin_core::RoleSelector {
        namespace: None,
        name: "deploy".to_owned(),
    }
}

fn test_manager<'a>(
    config: &'a jackin_config::AppConfig,
    cwd: &'a std::path::Path,
) -> ManagerState<'a> {
    ManagerState::from_config(config, cwd)
}

fn test_account_choice() -> crate::services::launch::AccountChoice {
    crate::services::launch::AccountChoice {
        id: "account-1".to_owned(),
        name: "Work".to_owned(),
        provider: jackin_config::AiProvider::OpenAi,
        agents: vec![jackin_core::Agent::Codex],
        configuration_id: None,
        instance_id: None,
    }
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
            name: "web".to_owned(),
            state: ConfirmState::new("Delete web?"),
        }
        .route(),
        ConsoleManagerStageRoute::ConfirmDelete
    );
    assert_eq!(
        ConsoleManagerStage::<(), (), ()>::ConfirmInstancePurge {
            container: "container-1".to_owned(),
            label: "web".to_owned(),
            state: ConfirmState::new("Purge web?"),
        }
        .route(),
        ConsoleManagerStageRoute::ConfirmInstancePurge
    );
    assert_eq!(
        ConsoleManagerStage::<(), (), WorkflowScreen>::Workflows(WorkflowScreen::default()).route(),
        ConsoleManagerStageRoute::Workflows
    );
}

#[test]
fn apply_manager_stage_updates_storage() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut manager = test_manager(&config, &cwd);
    manager.set_manager_stage(ManagerStage::ConfirmDelete {
        name: "web".to_owned(),
        state: ConfirmState::new("Delete web?"),
    });
    assert_eq!(
        manager.stage.route(),
        ConsoleManagerStageRoute::ConfirmDelete
    );
    apply_manager_stage(&mut manager, ManagerStage::List);
    assert_eq!(manager.stage.route(), ConsoleManagerStageRoute::List);
}

#[test]
fn open_launch_agent_prompt_plan_updates_app_and_manager() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    let role = test_role();
    let ConsoleAppStage::Manager(manager) = &mut app.stage;
    manager.inline_role_picker = Some(RolePickerState::launch(vec![role.clone()]));
    open_launch_agent_prompt_plan(
        &mut app,
        role.clone(),
        vec![jackin_core::Agent::Codex, jackin_core::Agent::Claude],
    );
    assert_eq!(app.pending_launch_role, Some(role));
    let ConsoleAppStage::Manager(manager) = &app.stage;
    assert!(manager.inline_agent_picker.is_some());
    assert!(manager.inline_role_picker.is_none());
}

#[test]
fn open_launch_role_prompt_plan_updates_app_and_manager() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    open_launch_role_prompt_plan(&mut app, "web".to_owned(), vec![test_role()], Some(0));
    assert_eq!(app.pending_launch, Some("web".to_owned()));
    assert_eq!(app.pending_launch_role, None);
    let ConsoleAppStage::Manager(manager) = &app.stage;
    assert!(manager.inline_role_picker.is_some());
}

#[test]
fn clear_pending_launch_plan_clears_launch_state() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    app.pending_launch = Some("web".to_owned());
    app.pending_launch_role = Some(test_role());
    clear_pending_launch_plan(&mut app);
    assert_eq!(app.pending_launch, None);
    assert_eq!(app.pending_launch_role, None);
}

#[test]
fn clear_pending_launch_role_plan_clears_only_role() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    app.pending_launch = Some("web".to_owned());
    app.pending_launch_role = Some(test_role());
    clear_pending_launch_role_plan(&mut app);
    assert_eq!(app.pending_launch_role, None);
    assert_eq!(app.pending_launch, Some("web".to_owned()));
}

#[test]
fn take_pending_launch_plan_takes_input() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    app.pending_launch = Some("web".to_owned());
    app.pending_launch_role = Some(test_role());
    assert_eq!(take_pending_launch_plan(&mut app), Some("web".to_owned()));
    assert_eq!(take_pending_launch_plan(&mut app), None);
    assert_eq!(app.pending_launch_role, Some(test_role()));
}

#[test]
fn take_pending_launch_and_role_plan_takes_pair() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    assert_eq!(take_pending_launch_and_role_plan(&mut app), None);
    app.pending_launch = Some("web".to_owned());
    assert_eq!(take_pending_launch_and_role_plan(&mut app), None);
    app.pending_launch = Some("web".to_owned());
    app.pending_launch_role = Some(test_role());
    assert_eq!(
        take_pending_launch_and_role_plan(&mut app),
        Some(("web".to_owned(), test_role()))
    );
    assert_eq!(app.pending_launch, None);
    assert_eq!(app.pending_launch_role, None);
}

#[test]
fn open_launch_account_picker_plan_updates_app_and_manager() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    let role = test_role();
    open_launch_account_picker_plan(
        &mut app,
        "db".to_owned(),
        role.clone(),
        jackin_core::Agent::Codex,
        vec![test_account_choice()],
    );
    assert_eq!(app.pending_launch, Some("db".to_owned()));
    assert_eq!(app.pending_launch_role, Some(role));
    let ConsoleAppStage::Manager(manager) = &app.stage;
    assert!(manager.launch_account_picker.is_some());
}

#[test]
fn console_app_base_surface_unblocked_respects_modal_blockers() {
    let config = jackin_config::AppConfig::default();
    let cwd = std::path::PathBuf::from("/tmp/jackin-console-test");
    let mut app: ConsoleApp<_, String, jackin_core::RoleSelector, ()> = ConsoleApp::new(
        ConsoleAppStage::Manager(test_manager(&config, &cwd)),
        (),
        false,
    );
    assert!(app.base_surface_unblocked());
    app.open_quit_confirm();
    assert!(app.quit_confirm_open());
    assert!(!app.base_surface_unblocked());
    app.dismiss_quit_confirm();
    assert!(app.base_surface_unblocked());
    let ConsoleAppStage::Manager(manager) = &mut app.stage;
    manager.open_error_popup_modal(crate::tui::components::ErrorPopupState::new(
        "Save failed",
        "disk full",
    ));
    assert!(!app.base_surface_unblocked());
    let ConsoleAppStage::Manager(manager) = &mut app.stage;
    manager.dismiss_list_modal();
    assert!(app.base_surface_unblocked());
}

struct RoleLoadFixture {
    next: Option<(String, anyhow::Result<()>)>,
}

impl ConsolePendingRoleLoad for RoleLoadFixture {
    type PendingRoleLoad = String;

    fn poll_pending_role_load(&mut self) -> Option<(String, anyhow::Result<()>)> {
        self.next.take()
    }
}

#[test]
fn console_manager_stage_polls_pending_role_load_from_editor_only() {
    let mut editor = ConsoleManagerStage::<(), RoleLoadFixture, ()>::Editor(RoleLoadFixture {
        next: Some(("role-key".to_owned(), Ok(()))),
    });
    let (pending, result) = editor
        .poll_pending_role_load()
        .expect("editor pending load");
    assert_eq!(pending, "role-key");
    result.unwrap();
    assert!(editor.poll_pending_role_load().is_none());
    let mut list = ConsoleManagerStage::<(), RoleLoadFixture, ()>::List;
    assert!(list.poll_pending_role_load().is_none());
    let mut settings = ConsoleManagerStage::<(), RoleLoadFixture, ()>::Settings(());
    assert!(settings.poll_pending_role_load().is_none());
}

struct DriftCheckFixture {
    next: Option<(String, anyhow::Result<String>)>,
}

impl ConsolePendingDriftCheck for DriftCheckFixture {
    type PendingDriftCheck = String;
    type DriftDetection = String;

    fn poll_pending_drift_check(&mut self) -> Option<(String, anyhow::Result<String>)> {
        self.next.take()
    }
}

#[test]
fn console_manager_stage_polls_pending_drift_check_from_editor_only() {
    let mut editor = ConsoleManagerStage::<(), DriftCheckFixture, ()>::Editor(DriftCheckFixture {
        next: Some(("save-plan".to_owned(), Ok("drift-summary".to_owned()))),
    });
    let (pending, result) = editor
        .poll_pending_drift_check()
        .expect("editor pending drift check");
    assert_eq!(pending, "save-plan");
    assert_eq!(result.expect("drift result"), "drift-summary");
    assert!(editor.poll_pending_drift_check().is_none());
    let mut list = ConsoleManagerStage::<(), DriftCheckFixture, ()>::List;
    assert!(list.poll_pending_drift_check().is_none());
    let mut settings = ConsoleManagerStage::<(), DriftCheckFixture, ()>::Settings(());
    assert!(settings.poll_pending_drift_check().is_none());
}

struct IsolationCleanupFixture {
    next: Option<(String, anyhow::Result<()>)>,
}

impl ConsolePendingIsolationCleanup for IsolationCleanupFixture {
    type PendingIsolationCleanup = String;

    fn poll_pending_isolation_cleanup(&mut self) -> Option<(String, anyhow::Result<()>)> {
        self.next.take()
    }
}

#[test]
fn console_manager_stage_polls_pending_isolation_cleanup_from_editor_only() {
    let mut editor =
        ConsoleManagerStage::<(), IsolationCleanupFixture, ()>::Editor(IsolationCleanupFixture {
            next: Some(("cleanup-plan".to_owned(), Ok(()))),
        });
    let (pending, result) = editor
        .poll_pending_isolation_cleanup()
        .expect("editor pending cleanup");
    assert_eq!(pending, "cleanup-plan");
    result.unwrap();
    assert!(editor.poll_pending_isolation_cleanup().is_none());
    let mut list = ConsoleManagerStage::<(), IsolationCleanupFixture, ()>::List;
    assert!(list.poll_pending_isolation_cleanup().is_none());
    let mut settings = ConsoleManagerStage::<(), IsolationCleanupFixture, ()>::Settings(());
    assert!(settings.poll_pending_isolation_cleanup().is_none());
}

struct OpCommitEditorFixture {
    next: Option<(String, anyhow::Result<()>)>,
}

impl ConsolePendingOpCommit for OpCommitEditorFixture {
    type OpRef = String;

    fn poll_pending_op_commit(&mut self) -> Option<(String, anyhow::Result<()>)> {
        self.next.take()
    }
}

struct OpCommitSettingsFixture {
    next: Option<(String, anyhow::Result<()>)>,
}

impl ConsolePendingOpCommit for OpCommitSettingsFixture {
    type OpRef = String;

    fn poll_pending_op_commit(&mut self) -> Option<(String, anyhow::Result<()>)> {
        self.next.take()
    }
}

#[test]
fn console_manager_stage_polls_pending_op_commit_with_origin() {
    let mut editor =
        ConsoleManagerStage::<(), OpCommitEditorFixture, OpCommitSettingsFixture>::Editor(
            OpCommitEditorFixture {
                next: Some(("op://vault/item/field".to_owned(), Ok(()))),
            },
        );
    let resolution = editor
        .poll_pending_op_commit()
        .expect("editor pending op commit");
    assert_eq!(resolution.op_ref, "op://vault/item/field");
    resolution.result.unwrap();
    assert!(matches!(
        resolution.origin,
        ConsolePendingOpCommitOrigin::Editor
    ));
    let mut settings =
        ConsoleManagerStage::<(), OpCommitEditorFixture, OpCommitSettingsFixture>::Settings(
            OpCommitSettingsFixture {
                next: Some((
                    "op://vault/item/other".to_owned(),
                    Err(anyhow::anyhow!("denied")),
                )),
            },
        );
    let resolution = settings
        .poll_pending_op_commit()
        .expect("settings pending op commit");
    assert_eq!(resolution.op_ref, "op://vault/item/other");
    assert!(resolution.result.is_err());
    assert!(matches!(
        resolution.origin,
        ConsolePendingOpCommitOrigin::Settings
    ));
    let mut list = ConsoleManagerStage::<(), OpCommitEditorFixture, OpCommitSettingsFixture>::List;
    assert!(list.poll_pending_op_commit().is_none());
}

struct DebugEditorFixture;

impl ConsoleEditorDebugFacts for DebugEditorFixture {
    fn editor_stage_debug(&self) -> ConsoleStageDebug {
        ConsoleStageDebug::Editor {
            mode: "general".to_owned(),
            tab: "mounts".to_owned(),
            field: "name".to_owned(),
            modal: None,
        }
    }
}

struct DebugSettingsFixture;

impl ConsoleSettingsDebugFacts for DebugSettingsFixture {
    fn settings_stage_debug(&self) -> ConsoleStageDebug {
        ConsoleStageDebug::Settings {
            tab: "env".to_owned(),
            selected: 2,
            modal: None,
        }
    }
}

struct DebugPreludeFixture;

impl ConsoleCreatePreludeDebugFacts for DebugPreludeFixture {
    fn create_prelude_stage_debug(&self) -> ConsoleStageDebug {
        ConsoleStageDebug::CreatePrelude {
            step: "NameWorkspace".to_owned(),
            modal: None,
        }
    }
}

#[test]
fn console_manager_stage_reports_debug_stage() {
    let list =
        ConsoleManagerStage::<DebugPreludeFixture, DebugEditorFixture, DebugSettingsFixture>::List;
    assert_eq!(list.debug_stage(), ConsoleStageDebug::List);
    let editor =
        ConsoleManagerStage::<DebugPreludeFixture, DebugEditorFixture, DebugSettingsFixture>::Editor(
            DebugEditorFixture,
        );
    assert_eq!(
        editor.debug_stage(),
        ConsoleStageDebug::Editor {
            mode: "general".to_owned(),
            tab: "mounts".to_owned(),
            field: "name".to_owned(),
            modal: None,
        }
    );
    let settings = ConsoleManagerStage::<
        DebugPreludeFixture,
        DebugEditorFixture,
        DebugSettingsFixture,
    >::Settings(DebugSettingsFixture);
    assert_eq!(
        settings.debug_stage(),
        ConsoleStageDebug::Settings {
            tab: "env".to_owned(),
            selected: 2,
            modal: None,
        }
    );
    let prelude = ConsoleManagerStage::<
        DebugPreludeFixture,
        DebugEditorFixture,
        DebugSettingsFixture,
    >::CreatePrelude(DebugPreludeFixture);
    assert_eq!(
        prelude.debug_stage(),
        ConsoleStageDebug::CreatePrelude {
            step: "NameWorkspace".to_owned(),
            modal: None,
        }
    );
}

struct HeightEditorFixture(u16);

impl ConsoleEditorFooterHeight for HeightEditorFixture {
    fn editor_cached_footer_height(&self) -> u16 {
        self.0
    }
}

struct HeightSettingsFixture(u16);

impl ConsoleSettingsFooterHeight for HeightSettingsFixture {
    fn settings_cached_footer_height(&self) -> u16 {
        self.0
    }
}

#[test]
fn console_manager_stage_reports_footer_height_facts() {
    let editor = ConsoleManagerStage::<(), HeightEditorFixture, HeightSettingsFixture>::Editor(
        HeightEditorFixture(7),
    );
    assert_eq!(
        editor.footer_height_facts(11),
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Editor,
            workspace_footer_height: 11,
            editor_footer_height: 7,
            settings_footer_height: 0,
        }
    );
    let settings = ConsoleManagerStage::<(), HeightEditorFixture, HeightSettingsFixture>::Settings(
        HeightSettingsFixture(5),
    );
    assert_eq!(
        settings.footer_height_facts(11),
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::Settings,
            workspace_footer_height: 11,
            editor_footer_height: 0,
            settings_footer_height: 5,
        }
    );
    let list = ConsoleManagerStage::<(), HeightEditorFixture, HeightSettingsFixture>::List;
    assert_eq!(
        list.footer_height_facts(11),
        StageFooterHeightFacts {
            route: ConsoleManagerStageRoute::List,
            workspace_footer_height: 11,
            editor_footer_height: 0,
            settings_footer_height: 0,
        }
    );
}
