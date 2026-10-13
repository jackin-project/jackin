// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn open_launch_account_picker_plan_updates_app_and_manager() {
    let mut app: ConsoleApp<TestLaunchPromptManager, &'static str, TestPromptRole, ()> =
        ConsoleApp::new(
            ConsoleAppStage::Manager(TestLaunchPromptManager::default()),
            (),
            false,
        );

    open_launch_account_picker_plan(
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
        manager.account_picker_role,
        Some(TestPromptRole("architect"))
    );
    assert_eq!(
        manager.account_picker_agent,
        Some(jackin_core::Agent::Claude)
    );
    assert_eq!(manager.account_picker_providers, vec!["anthropic", "zai"]);
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
            wizard: create_prelude_wizard_state(),
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
