// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
    assert_eq!(resolution.origin, ConsolePendingOpCommitOrigin::Editor);
    assert!(editor.poll_pending_op_commit().is_none());

    let mut settings = Stage::Settings(TestOpCommit {
        pending: Some((5, Ok(()))),
    });
    let Some(resolution) = settings.poll_pending_op_commit() else {
        panic!("expected pending settings op commit");
    };
    assert_eq!(resolution.op_ref, 5);
    resolution.result.unwrap();
    assert_eq!(resolution.origin, ConsolePendingOpCommitOrigin::Settings);
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
            wizard: create_prelude_wizard_state(),
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
        stage_route: ConsoleManagerStageRoute::Settings,
    };

    assert_eq!(
        console_input_dispatch_plan(base),
        ConsoleInputDispatchPlan::Stage(ConsoleManagerStageRoute::Settings)
    );
    // The help overlay outranks every modal/picker arm.
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            keyboard_help_open: true,
            list_modal_open: true,
            editor_modal_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::KeyboardHelp
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
            inline_account_picker_open: true,
            launch_account_picker_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::InlineAccountPicker
    );
    assert_eq!(
        console_input_dispatch_plan(ConsoleInputDispatchFacts {
            launch_account_picker_open: true,
            inline_agent_picker_open: true,
            ..base
        }),
        ConsoleInputDispatchPlan::LaunchAccountPicker
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
