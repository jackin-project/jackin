// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `update`.
use super::*;
use ratatui::layout::Rect;

#[test]
fn settings_tab_move_plan_cycles_and_sets_focus() {
    assert_eq!(
        settings_tab_move_plan(SettingsTab::Trust, 1, true),
        SettingsTabMovePlan {
            active_tab: SettingsTab::General,
            tab_bar_focused: true,
        }
    );
    assert_eq!(
        settings_tab_move_plan(SettingsTab::General, -1, false),
        SettingsTabMovePlan {
            active_tab: SettingsTab::Trust,
            tab_bar_focused: false,
        }
    );
}

#[test]
fn settings_tab_select_plan_focuses_selected_tab() {
    assert_eq!(
        settings_tab_select_plan(SettingsTab::Trust),
        SettingsTabMovePlan {
            active_tab: SettingsTab::Trust,
            tab_bar_focused: true,
        }
    );
}

#[test]
fn settings_tab_bar_focus_plan_returns_requested_focus() {
    assert!(settings_tab_bar_focus_plan(true));
    assert!(!settings_tab_bar_focus_plan(false));
}

#[test]
fn settings_shell_key_plan_routes_tab_shell_keys_from_facts() {
    assert_eq!(
        settings_shell_key_plan(KeyCode::Left, true, false),
        SettingsShellKeyPlan::MoveTab {
            delta: -1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Right, true, false),
        SettingsShellKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Down, true, false),
        SettingsShellKeyPlan::FocusContent
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Char('J'), true, false),
        SettingsShellKeyPlan::FocusContent
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Tab, false, false),
        SettingsShellKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::BackTab, false, false),
        SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: false,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Esc, false, true),
        SettingsShellKeyPlan::FocusTabBar {
            clear_auth_kind: true,
        }
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Char('s'), true, false),
        SettingsShellKeyPlan::Continue
    );
    assert_eq!(
        settings_shell_key_plan(KeyCode::Esc, true, true),
        SettingsShellKeyPlan::Continue
    );
}

#[test]
fn settings_general_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_general_key_plan(KeyCode::Up, false),
        SettingsGeneralKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('J'), false),
        SettingsGeneralKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char(' '), false),
        SettingsGeneralKeyPlan::ToggleSelected
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Esc, true),
        SettingsGeneralKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Esc, false),
        SettingsGeneralKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('q'), true),
        SettingsGeneralKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('S'), false),
        SettingsGeneralKeyPlan::Save
    );
    assert_eq!(
        settings_general_key_plan(KeyCode::Char('x'), false),
        SettingsGeneralKeyPlan::Noop
    );
}

#[test]
fn settings_env_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_env_key_plan(KeyCode::Up, true, false, false, false),
        SettingsEnvKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('J'), true, false, false, false),
        SettingsEnvKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Esc, true, true, false, false),
        SettingsEnvKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Esc, true, false, false, false),
        SettingsEnvKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('a'), true, false, false, false),
        SettingsEnvKeyPlan::OpenAdd
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('S'), true, false, false, false),
        SettingsEnvKeyPlan::Save
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('d'), true, false, false, false),
        SettingsEnvKeyPlan::ConfirmDelete
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('d'), false, false, false, false),
        SettingsEnvKeyPlan::Noop
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('m'), true, false, false, false),
        SettingsEnvKeyPlan::ToggleMask
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('p'), true, false, true, false),
        SettingsEnvKeyPlan::OpenPicker
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('p'), true, false, false, false),
        SettingsEnvKeyPlan::Noop
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Enter, true, false, true, true),
        SettingsEnvKeyPlan::OpenPicker
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Enter, true, false, false, true),
        SettingsEnvKeyPlan::OpenEnterModal
    );
    assert_eq!(
        settings_env_key_plan(KeyCode::Char('x'), true, false, true, true),
        SettingsEnvKeyPlan::Noop
    );
}

#[test]
fn settings_auth_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_auth_key_plan(KeyCode::Esc, true, true, true),
        SettingsAuthKeyPlan::ClearKind
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('Q'), true, true, false),
        SettingsAuthKeyPlan::ClearKind
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Up, false, false, false),
        SettingsAuthKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('J'), false, true, false),
        SettingsAuthKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Enter, false, false, false),
        SettingsAuthKeyPlan::EnterKind
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Esc, true, false, false),
        SettingsAuthKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Esc, false, false, false),
        SettingsAuthKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Enter, false, true, true),
        SettingsAuthKeyPlan::OpenForm
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Enter, false, true, false),
        SettingsAuthKeyPlan::Noop
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('s'), false, true, false),
        SettingsAuthKeyPlan::Save
    );
    assert_eq!(
        settings_auth_key_plan(KeyCode::Char('d'), false, true, true),
        SettingsAuthKeyPlan::Noop
    );
}

#[test]
fn settings_env_header_key_plan_routes_role_header_arrows() {
    let collapsed = SettingsEnvRow::RoleHeader {
        role: "ops".to_owned(),
        expanded: false,
    };
    let expanded = SettingsEnvRow::RoleHeader {
        role: "ops".to_owned(),
        expanded: true,
    };
    let key_row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "TOKEN".to_owned(),
    };

    assert_eq!(
        settings_env_header_key_plan(KeyCode::Right, SettingsTab::Environments, Some(&collapsed),),
        SettingsEnvHeaderKeyPlan::SetExpanded {
            role: "ops".to_owned(),
            expanded: true,
        }
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Left, SettingsTab::Environments, Some(&expanded)),
        SettingsEnvHeaderKeyPlan::SetExpanded {
            role: "ops".to_owned(),
            expanded: false,
        }
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Right, SettingsTab::Environments, Some(&expanded)),
        SettingsEnvHeaderKeyPlan::Consume
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Left, SettingsTab::Environments, Some(&key_row)),
        SettingsEnvHeaderKeyPlan::Consume
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Right, SettingsTab::General, Some(&collapsed)),
        SettingsEnvHeaderKeyPlan::Continue
    );
    assert_eq!(
        settings_env_header_key_plan(KeyCode::Enter, SettingsTab::Environments, Some(&collapsed)),
        SettingsEnvHeaderKeyPlan::Continue
    );
}

#[test]
fn settings_env_selected_header_key_plan_uses_current_flat_selection() {
    let pending = env_config();
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::RoleHeader {
                    role,
                    expanded: false,
                } if role == "alpha"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_header_key_plan(
            KeyCode::Right,
            SettingsTab::Environments,
            &pending,
            &expanded,
            selected,
        ),
        SettingsEnvHeaderKeyPlan::SetExpanded {
            role: "alpha".to_owned(),
            expanded: true,
        }
    );
}

#[test]
fn settings_top_level_key_plan_applies_shell_before_header_and_delegates() {
    let pending = env_config();
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let role_header = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::RoleHeader {
                    role,
                    expanded: false,
                } if role == "alpha"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_top_level_key_plan(
            KeyCode::Tab,
            SettingsTab::Environments,
            false,
            false,
            &pending,
            &expanded,
            role_header,
        ),
        SettingsTopLevelKeyPlan::MoveTab {
            delta: 1,
            focus_tab_bar: true,
        }
    );
    assert_eq!(
        settings_top_level_key_plan(
            KeyCode::Right,
            SettingsTab::Environments,
            false,
            false,
            &pending,
            &expanded,
            role_header,
        ),
        SettingsTopLevelKeyPlan::SetEnvRoleExpanded {
            role: "alpha".to_owned(),
            expanded: true,
        }
    );
    assert_eq!(
        settings_top_level_key_plan(
            KeyCode::Char('s'),
            SettingsTab::Trust,
            false,
            false,
            &pending,
            &expanded,
            role_header,
        ),
        SettingsTopLevelKeyPlan::Delegate(SettingsTab::Trust)
    );
}

#[test]
fn settings_trust_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_trust_key_plan(KeyCode::Up, false),
        SettingsTrustKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Char('J'), false),
        SettingsTrustKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Char('h'), false),
        SettingsTrustKeyPlan::ScrollHorizontal { delta: -8 }
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Char('L'), false),
        SettingsTrustKeyPlan::ScrollHorizontal { delta: 8 }
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Char(' '), false),
        SettingsTrustKeyPlan::ToggleSelected
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Esc, true),
        SettingsTrustKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Esc, false),
        SettingsTrustKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Char('q'), true),
        SettingsTrustKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Char('S'), false),
        SettingsTrustKeyPlan::Save
    );
    assert_eq!(
        settings_trust_key_plan(KeyCode::Char('x'), false),
        SettingsTrustKeyPlan::Noop
    );
}

#[test]
fn global_mount_text_commit_plan_routes_targets_and_trims_values() {
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::AddScope, " ops "),
        GlobalMountTextCommitPlan::AddScope(Some("ops".to_owned()))
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::AddScope, " "),
        GlobalMountTextCommitPlan::AddScope(None)
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::AddName, " cache "),
        GlobalMountTextCommitPlan::AddName("cache".to_owned())
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::AddSource, " /tmp/data "),
        GlobalMountTextCommitPlan::AddSource("/tmp/data".to_owned())
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::AddDestination, " /jackin/data "),
        GlobalMountTextCommitPlan::AddDestination("/jackin/data".to_owned())
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::Source, " /tmp/src "),
        GlobalMountTextCommitPlan::SetSource("/tmp/src".to_owned())
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::Destination, " /dst "),
        GlobalMountTextCommitPlan::SetDestination("/dst".to_owned())
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::Scope, " role "),
        GlobalMountTextCommitPlan::SetScope(Some("role".to_owned()))
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::Scope, " "),
        GlobalMountTextCommitPlan::SetScope(None)
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::Rename, " renamed "),
        GlobalMountTextCommitPlan::Rename("renamed".to_owned())
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::Rename, " "),
        GlobalMountTextCommitPlan::EmptyName
    );
    assert_eq!(
        global_mount_text_commit_plan(&GlobalMountTextTarget::AddName, " "),
        GlobalMountTextCommitPlan::EmptyName
    );
}

#[test]
fn global_mount_add_finalize_plan_validates_and_builds_row() {
    let empty_dst = GlobalMountDraft {
        name: String::new(),
        src: "/host/cache".to_owned(),
        dst: " ".to_owned(),
        scope: Some("ops".to_owned()),
    };
    assert_eq!(
        global_mount_add_finalize_plan(&[], empty_dst.clone()),
        GlobalMountAddFinalizePlan::EmptyDestination(empty_dst)
    );

    let pending = vec![jackin_config::GlobalMountRow {
        scope: Some("ops".to_owned()),
        name: "cache".to_owned(),
        mount: crate::services::workspace::shared_mount_config(
            "/host/old".to_owned(),
            "/jackin/cache".to_owned(),
            false,
        ),
    }];
    let draft = GlobalMountDraft {
        name: String::new(),
        src: "/host/cache".to_owned(),
        dst: "/jackin/cache".to_owned(),
        scope: Some("ops".to_owned()),
    };
    let plan = global_mount_add_finalize_plan(&pending, draft);
    assert!(matches!(plan, GlobalMountAddFinalizePlan::Add { .. }));
    if let GlobalMountAddFinalizePlan::Add { row, selected } = plan {
        assert_eq!(selected, 1);
        assert_eq!(row.scope.as_deref(), Some("ops"));
        assert_eq!(row.name, "cache-2");
        assert_eq!(row.mount.src, "/host/cache");
        assert_eq!(row.mount.dst, "/jackin/cache");
        assert!(!row.mount.readonly);
    }
}

#[test]
fn global_mount_add_finalize_apply_plan_owns_draft_lifecycle() {
    let mut missing = None;
    assert_eq!(
        global_mount_add_finalize_apply_plan(&[], &mut missing),
        GlobalMountAddFinalizeApplyPlan::MissingDraft
    );

    let empty_draft = GlobalMountDraft {
        name: String::new(),
        src: "/host/cache".to_owned(),
        dst: " ".to_owned(),
        scope: Some("ops".to_owned()),
    };
    let mut empty = Some(empty_draft.clone());
    assert_eq!(
        global_mount_add_finalize_apply_plan(&[], &mut empty),
        GlobalMountAddFinalizeApplyPlan::EmptyDestination
    );
    assert_eq!(empty, Some(empty_draft));

    let mut valid = Some(GlobalMountDraft {
        name: String::new(),
        src: "/host/cache".to_owned(),
        dst: "/jackin/cache".to_owned(),
        scope: Some("ops".to_owned()),
    });
    let plan = global_mount_add_finalize_apply_plan(&[], &mut valid);
    assert!(valid.is_none());
    assert!(matches!(plan, GlobalMountAddFinalizeApplyPlan::Add { .. }));
}

#[test]
fn set_global_mount_add_draft_destination_updates_existing_draft() {
    let mut draft = Some(GlobalMountDraft::default());
    assert!(set_global_mount_add_draft_destination(
        &mut draft,
        "/jackin/cache",
    ));
    assert_eq!(
        draft.as_ref().map(|draft| draft.dst.as_str()),
        Some("/jackin/cache")
    );

    let mut missing = None;
    assert!(!set_global_mount_add_draft_destination(
        &mut missing,
        "/jackin/cache",
    ));
}

#[test]
fn global_mount_add_text_apply_plan_updates_draft_and_routes_next_step() {
    let mut draft = Some(GlobalMountDraft::default());
    assert_eq!(
        global_mount_add_text_apply_plan(
            &mut draft,
            GlobalMountTextCommitPlan::AddScope(Some("ops".to_owned())),
        ),
        GlobalMountAddTextApplyPlan::OpenFileBrowser
    );
    assert_eq!(
        draft.as_ref().and_then(|draft| draft.scope.clone()),
        Some("ops".to_owned())
    );

    assert_eq!(
        global_mount_add_text_apply_plan(
            &mut draft,
            GlobalMountTextCommitPlan::AddName("cache".to_owned()),
        ),
        GlobalMountAddTextApplyPlan::OpenAddSource
    );
    assert_eq!(
        draft.as_ref().map(|draft| draft.name.as_str()),
        Some("cache")
    );

    assert_eq!(
        global_mount_add_text_apply_plan(
            &mut draft,
            GlobalMountTextCommitPlan::AddSource("/host/cache".to_owned()),
        ),
        GlobalMountAddTextApplyPlan::OpenAddDestination
    );
    assert_eq!(
        draft.as_ref().map(|draft| draft.src.as_str()),
        Some("/host/cache")
    );

    assert_eq!(
        global_mount_add_text_apply_plan(
            &mut draft,
            GlobalMountTextCommitPlan::AddDestination("/jackin/cache".to_owned()),
        ),
        GlobalMountAddTextApplyPlan::Finalize
    );
    assert_eq!(
        draft.as_ref().map(|draft| draft.dst.as_str()),
        Some("/jackin/cache")
    );

    assert_eq!(
        global_mount_add_text_apply_plan(&mut None, GlobalMountTextCommitPlan::AddName("x".into())),
        GlobalMountAddTextApplyPlan::MissingDraft
    );
    assert_eq!(
        global_mount_add_text_apply_plan(&mut draft, GlobalMountTextCommitPlan::Rename("x".into())),
        GlobalMountAddTextApplyPlan::Noop
    );
}

#[test]
fn global_mount_edit_text_apply_plan_updates_selected_row() {
    let mut rows = vec![jackin_config::GlobalMountRow {
        scope: Some("ops".to_owned()),
        name: "cache".to_owned(),
        mount: jackin_config::MountConfig {
            src: "/host/cache".to_owned(),
            dst: "/jackin/cache".to_owned(),
            readonly: false,
            isolation: jackin_config::MountIsolation::Shared,
        },
    }];

    assert_eq!(
        global_mount_edit_text_apply_plan(
            &mut rows,
            0,
            GlobalMountTextCommitPlan::SetSource("/host/new".to_owned()),
        ),
        GlobalMountEditTextApplyPlan::Applied
    );
    assert_eq!(rows[0].mount.src, "/host/new");

    assert_eq!(
        global_mount_edit_text_apply_plan(
            &mut rows,
            0,
            GlobalMountTextCommitPlan::SetDestination("/jackin/new".to_owned()),
        ),
        GlobalMountEditTextApplyPlan::Applied
    );
    assert_eq!(rows[0].mount.dst, "/jackin/new");

    assert_eq!(
        global_mount_edit_text_apply_plan(&mut rows, 0, GlobalMountTextCommitPlan::SetScope(None),),
        GlobalMountEditTextApplyPlan::Applied
    );
    assert_eq!(rows[0].scope, None);

    assert_eq!(
        global_mount_edit_text_apply_plan(
            &mut rows,
            0,
            GlobalMountTextCommitPlan::Rename("renamed".to_owned()),
        ),
        GlobalMountEditTextApplyPlan::Applied
    );
    assert_eq!(rows[0].name, "renamed");
}

#[test]
fn global_mount_edit_text_apply_plan_reports_missing_and_non_edit_cases() {
    let mut rows = Vec::new();

    assert_eq!(
        global_mount_edit_text_apply_plan(
            &mut rows,
            0,
            GlobalMountTextCommitPlan::SetSource("/host/new".to_owned()),
        ),
        GlobalMountEditTextApplyPlan::MissingRow
    );
    assert_eq!(
        global_mount_edit_text_apply_plan(&mut rows, 0, GlobalMountTextCommitPlan::EmptyName),
        GlobalMountEditTextApplyPlan::EmptyName
    );
    assert_eq!(
        global_mount_edit_text_apply_plan(
            &mut rows,
            0,
            GlobalMountTextCommitPlan::AddName("cache".to_owned()),
        ),
        GlobalMountEditTextApplyPlan::Noop
    );
}

#[test]
fn settings_global_mounts_key_plan_routes_keys_from_facts() {
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('s'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenSavePreview
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('s'), false, true, 0, 0),
        SettingsGlobalMountsKeyPlan::ConfirmSensitiveSave
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('h'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ScrollHorizontal { delta: -8 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('l'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ScrollHorizontal { delta: 8 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Up, false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::MoveSelection { delta: -1 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Down, false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::MoveSelection { delta: 1 }
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('r'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ToggleReadonly
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Esc, true, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ConfirmDiscard
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Esc, false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::ReturnToList
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Enter, false, false, 2, 2),
        SettingsGlobalMountsKeyPlan::OpenAdd
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Enter, false, false, 1, 2),
        SettingsGlobalMountsKeyPlan::Noop
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('a'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenAdd
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('d'), false, false, 0, 1),
        SettingsGlobalMountsKeyPlan::ConfirmRemove
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('d'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::Noop
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('o'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenGithub
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('n'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Rename)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('1'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Source)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('2'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Destination)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('3'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::OpenEdit(GlobalMountTextTarget::Scope)
    );
    assert_eq!(
        settings_global_mounts_key_plan(KeyCode::Char('x'), false, false, 0, 0),
        SettingsGlobalMountsKeyPlan::Noop
    );
}

#[test]
fn global_mount_scope_picker_commit_plan_routes_choices() {
    assert_eq!(
        global_mount_scope_picker_commit_plan(ScopeChoice::AllAgents),
        GlobalMountScopePickerCommitPlan::ApplyAllAgentsScope
    );
    assert_eq!(
        global_mount_scope_picker_commit_plan(ScopeChoice::SpecificAgent),
        GlobalMountScopePickerCommitPlan::OpenRolePicker
    );
}

#[test]
fn global_mount_role_picker_roles_parse_trust_rows() {
    let rows = vec![
        SettingsTrustRow {
            role: "ops".to_owned(),
            git: "https://example.invalid/ops.git".to_owned(),
            trusted: true,
        },
        SettingsTrustRow {
            role: "chainargos/agent-brown".to_owned(),
            git: "https://example.invalid/brown.git".to_owned(),
            trusted: false,
        },
    ];

    let keys = global_mount_role_picker_roles(&rows)
        .into_iter()
        .map(|role| role.key())
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec!["ops".to_owned(), "chainargos/agent-brown".to_owned()]
    );
}

#[test]
fn global_mount_role_picker_open_plan_requires_roles() {
    assert_eq!(
        global_mount_role_picker_open_plan(&[]),
        RolePickerOpenPlan::NoRoles
    );

    let rows = vec![SettingsTrustRow {
        role: "ops".to_owned(),
        git: "https://example.invalid/ops.git".to_owned(),
        trusted: true,
    }];
    assert!(matches!(
        global_mount_role_picker_open_plan(&rows),
        RolePickerOpenPlan::Open(roles) if roles.len() == 1 && roles[0].key() == "ops"
    ));
}

#[test]
fn global_mount_role_picker_commit_plan_sets_draft_scope() {
    let role = RoleSelector::parse("ops").unwrap();
    let mut draft = Some(GlobalMountDraft::default());
    assert_eq!(
        global_mount_role_picker_commit_plan(&mut draft, &role),
        GlobalMountRolePickerCommitPlan::OpenFileBrowser
    );
    assert_eq!(
        draft.as_ref().and_then(|draft| draft.scope.clone()),
        Some("ops".to_owned())
    );

    let mut missing = None;
    assert_eq!(
        global_mount_role_picker_commit_plan(&mut missing, &role),
        GlobalMountRolePickerCommitPlan::MissingDraft
    );
}

#[test]
fn global_mount_github_open_plan_uses_selected_row_cache_entry() {
    let rows = vec![
        jackin_config::GlobalMountRow {
            scope: None,
            name: "plain".to_owned(),
            mount: jackin_config::MountConfig {
                src: "/plain".to_owned(),
                dst: "/jackin/plain".to_owned(),
                readonly: false,
                isolation: jackin_config::MountIsolation::Shared,
            },
        },
        jackin_config::GlobalMountRow {
            scope: Some("ops".to_owned()),
            name: "repo".to_owned(),
            mount: jackin_config::MountConfig {
                src: "/repo".to_owned(),
                dst: "/jackin/repo".to_owned(),
                readonly: true,
                isolation: jackin_config::MountIsolation::Shared,
            },
        },
    ];
    let cache = crate::mount_info_cache::MountInfoCache::default();
    cache.store_entries([
        ("/plain".to_owned(), crate::mount_info::MountKind::Folder),
        (
            "/repo".to_owned(),
            crate::mount_info::MountKind::Git {
                branch: crate::mount_info::GitBranch::Named("main".to_owned()),
                origin: Some(crate::mount_info::GitOrigin::Github {
                    remote_url: "git@github.com:owner/repo.git".to_owned(),
                    web_url: "https://github.com/owner/repo/tree/main".to_owned(),
                }),
            },
        ),
    ]);

    assert_eq!(
        global_mount_github_open_plan(&rows, 0, &cache),
        GlobalMountGithubOpenPlan::NoGithubUrl
    );
    assert_eq!(
        global_mount_github_open_plan(&rows, 1, &cache),
        GlobalMountGithubOpenPlan::Open("https://github.com/owner/repo/tree/main".to_owned())
    );
    assert_eq!(
        global_mount_github_open_plan(&rows, 2, &cache),
        GlobalMountGithubOpenPlan::NoSelection
    );
}

#[test]
fn settings_env_text_commit_plan_routes_keys_and_values() {
    let role_scope = SettingsEnvScope::Role("ops".to_owned());
    let target = SettingsEnvTextTarget::EnvKey {
        scope: role_scope.clone(),
    };
    assert_eq!(
        settings_env_text_commit_plan(&target, " ", false),
        SettingsEnvTextCommitPlan::EmptyKey {
            scope: role_scope.clone(),
        }
    );
    assert_eq!(
        settings_env_text_commit_plan(&target, " TOKEN ", true),
        SettingsEnvTextCommitPlan::SetPendingPickerValue {
            scope: role_scope.clone(),
            key: "TOKEN".to_owned(),
        }
    );
    assert_eq!(
        settings_env_text_commit_plan(&target, " TOKEN ", false),
        SettingsEnvTextCommitPlan::OpenSourcePicker {
            scope: role_scope.clone(),
            key: "TOKEN".to_owned(),
        }
    );
    assert_eq!(
        settings_env_text_commit_plan(
            &SettingsEnvTextTarget::EnvValue {
                scope: SettingsEnvScope::Global,
                key: "TOKEN".to_owned(),
            },
            " value with spaces ",
            false,
        ),
        SettingsEnvTextCommitPlan::SetPlainValue {
            scope: SettingsEnvScope::Global,
            key: "TOKEN".to_owned(),
            value: " value with spaces ".to_owned(),
        }
    );
}

#[test]
fn settings_env_source_picker_commit_plan_requires_pending_key() {
    assert_eq!(
        settings_env_source_picker_commit_plan(SettingsEnvSourcePickerSelection::Plain, None),
        SettingsEnvSourcePickerCommitPlan::MissingPendingKey
    );

    let pending = (SettingsEnvScope::Role("ops".to_owned()), "TOKEN".to_owned());
    assert_eq!(
        settings_env_source_picker_commit_plan(
            SettingsEnvSourcePickerSelection::Plain,
            Some(&pending),
        ),
        SettingsEnvSourcePickerCommitPlan::OpenPlainText {
            scope: SettingsEnvScope::Role("ops".to_owned()),
            key: "TOKEN".to_owned(),
        }
    );
    assert_eq!(
        settings_env_source_picker_commit_plan(
            SettingsEnvSourcePickerSelection::Op,
            Some(&pending)
        ),
        SettingsEnvSourcePickerCommitPlan::OpenOpPicker {
            scope: SettingsEnvScope::Role("ops".to_owned()),
            key: "TOKEN".to_owned(),
        }
    );
}

#[test]
fn settings_env_op_picker_commit_plan_routes_targets() {
    assert_eq!(
        settings_env_op_picker_commit_plan(None),
        SettingsEnvOpPickerCommitPlan::MissingTarget
    );

    let existing = (
        SettingsEnvScope::Role("ops".to_owned()),
        Some("TOKEN".to_owned()),
    );
    assert_eq!(
        settings_env_op_picker_commit_plan(Some(&existing)),
        SettingsEnvOpPickerCommitPlan::SetExisting {
            scope: SettingsEnvScope::Role("ops".to_owned()),
            key: "TOKEN".to_owned(),
        }
    );

    let new_key = (SettingsEnvScope::Global, None);
    assert_eq!(
        settings_env_op_picker_commit_plan(Some(&new_key)),
        SettingsEnvOpPickerCommitPlan::StashForNewKey {
            scope: SettingsEnvScope::Global,
        }
    );
}

#[test]
fn settings_env_scope_picker_commit_plan_routes_scope_choices() {
    assert_eq!(
        settings_env_scope_picker_commit_plan(SettingsEnvScopePickerSelection::AllAgents),
        SettingsEnvScopePickerCommitPlan::OpenGlobalKeyInput {
            scope: SettingsEnvScope::Global,
        }
    );
    assert_eq!(
        settings_env_scope_picker_commit_plan(SettingsEnvScopePickerSelection::SpecificAgent),
        SettingsEnvScopePickerCommitPlan::OpenRolePicker
    );
}

#[test]
fn settings_env_role_picker_commit_plan_maps_role_to_scope() {
    let role = RoleSelector::parse("ops").unwrap();
    assert_eq!(
        settings_env_role_picker_commit_plan(&role),
        SettingsEnvRolePickerCommitPlan {
            scope: SettingsEnvScope::Role("ops".to_owned()),
        }
    );
}

#[test]
fn settings_env_role_picker_roles_parse_registered_roles() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::from([
            ("ops".to_owned(), BTreeMap::<String, &'static str>::new()),
            ("chainargos/agent-brown".to_owned(), BTreeMap::new()),
        ]),
    };

    let keys = settings_env_role_picker_roles(&pending)
        .into_iter()
        .map(|role| role.key())
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec!["chainargos/agent-brown".to_owned(), "ops".to_owned()]
    );
}

#[test]
fn settings_env_role_picker_open_plan_requires_roles() {
    let empty = SettingsEnvConfig::<&'static str> {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    };
    assert_eq!(
        settings_env_role_picker_open_plan(&empty),
        RolePickerOpenPlan::NoRoles
    );

    let pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::from([("ops".to_owned(), BTreeMap::<String, &'static str>::new())]),
    };
    assert!(matches!(
        settings_env_role_picker_open_plan(&pending),
        RolePickerOpenPlan::Open(roles) if roles.len() == 1 && roles[0].key() == "ops"
    ));
}

#[test]
fn settings_tab_at_position_maps_tab_strip_cells() {
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(SettingsTab::General)
    );
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT, 11),
        Some(SettingsTab::Mounts)
    );
    assert_eq!(
        settings_tab_at_position(crate::tui::layout::SCREEN_HEADER_HEIGHT - 1, 1),
        None
    );
}

#[test]
fn settings_tab_hover_plan_maps_strip() {
    assert_eq!(
        settings_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(0)
    );
    assert_eq!(
        settings_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT, 11),
        Some(1)
    );
    assert_eq!(
        settings_tab_hover_plan(crate::tui::layout::SCREEN_HEADER_HEIGHT - 1, 1),
        None
    );
}

#[test]
fn settings_tab_hover_target_plan_maps_strip_without_blocking_modals() {
    assert_eq!(
        settings_tab_hover_target_plan(false, false, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        Some(SettingsHoverTarget::Tab(0))
    );
    assert_eq!(
        settings_tab_hover_target_plan(true, false, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        None
    );
    assert_eq!(
        settings_tab_hover_target_plan(false, true, crate::tui::layout::SCREEN_HEADER_HEIGHT, 1),
        None
    );
}

#[test]
fn settings_auth_detail_row_count_adds_source_row_only_when_needed() {
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Github, AuthMode::Token),
        3
    );
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Github, AuthMode::Sync),
        2
    );
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Claude, AuthMode::Sync),
        3
    );
    assert_eq!(
        settings_auth_detail_row_count(AuthKind::Claude, AuthMode::ApiKey),
        3
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestAuthKind {
    Claude,
}

#[test]
fn settings_auth_kind_entry_plan_selects_kind_and_resets_row() {
    assert_eq!(
        enter_settings_auth_kind_plan(Some(TestAuthKind::Claude)),
        Some(SettingsAuthKindPlan {
            selected_kind: Some(TestAuthKind::Claude),
            selected: 0,
        })
    );
    assert_eq!(enter_settings_auth_kind_plan::<TestAuthKind>(None), None);
}

#[test]
fn settings_auth_kind_clear_plan_clears_kind_and_resets_row() {
    assert_eq!(
        clear_settings_auth_kind_plan::<TestAuthKind>(),
        SettingsAuthKindPlan {
            selected_kind: None,
            selected: 0,
        }
    );
}

#[test]
fn settings_auth_selection_plan_clamps_to_rows() {
    let rows = [
        SettingsAuthDetailRow::Mode,
        SettingsAuthDetailRow::Source,
        SettingsAuthDetailRow::SourceFolder,
        SettingsAuthDetailRow::Spacer,
    ];
    assert_eq!(settings_auth_selection_plan(0, &rows, 99), 0);
    assert_eq!(settings_auth_selection_plan(2, &rows, -99), 0);
}

#[test]
fn settings_auth_row_is_focusable_keeps_preview_rows_visible_but_skipped() {
    assert!(settings_auth_row_is_focusable(SettingsAuthDetailRow::Mode));
    assert!(!settings_auth_row_is_focusable(
        SettingsAuthDetailRow::Source
    ));
    assert!(!settings_auth_row_is_focusable(
        SettingsAuthDetailRow::SourceFolder
    ));
    assert!(!settings_auth_row_is_focusable(
        SettingsAuthDetailRow::Spacer
    ));
}

#[test]
fn settings_trust_selection_plan_clamps_and_updates_scroll() {
    let plan = settings_trust_selection_plan(0, 4, 99, 0, 8, 0);
    assert_eq!(plan.selected, 3);
    assert!(plan.scroll_y > 0);
}

#[test]
fn settings_trust_row_select_plan_bounds_checks_and_focuses_content() {
    assert_eq!(
        settings_trust_row_select_plan(1, 3),
        SettingsTrustRowSelectPlan {
            selected: Some(1),
            content_focused: true,
        }
    );
    assert_eq!(
        settings_trust_row_select_plan(3, 3),
        SettingsTrustRowSelectPlan {
            selected: None,
            content_focused: true,
        }
    );
}

#[test]
fn settings_trust_row_at_position_skips_header_and_applies_scroll() {
    let area = Rect::new(0, 5, 80, 10);

    assert_eq!(settings_trust_row_at_position(area, 1, 7, 0, 3), Some(0));
    assert_eq!(settings_trust_row_at_position(area, 1, 8, 2, 5), Some(3));
    assert_eq!(settings_trust_row_at_position(area, 1, 6, 0, 3), None);
    assert_eq!(settings_trust_row_at_position(area, 1, 10, 0, 3), None);
    assert_eq!(settings_trust_row_at_position(area, 80, 6, 0, 3), None);
}

#[test]
fn settings_trust_hover_target_at_position_maps_trust_rows() {
    let area = Rect::new(0, 5, 80, 10);

    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Trust, false, area, 1, 7, 0, 3),
        Some(SettingsHoverTarget::TrustRow(0))
    );
    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Trust, false, area, 1, 8, 2, 5),
        Some(SettingsHoverTarget::TrustRow(3))
    );
    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Mounts, false, area, 1, 7, 0, 3),
        None
    );
    assert_eq!(
        settings_trust_hover_target_at_position(SettingsTab::Trust, true, area, 1, 7, 0, 3),
        None
    );
}

#[test]
fn settings_trust_clickable_at_position_requires_trust_content_without_modal() {
    let area = Rect::new(0, 5, 80, 10);

    assert!(settings_trust_clickable_at_position(
        SettingsTab::Trust,
        false,
        area,
        1,
        6,
    ));
    assert!(!settings_trust_clickable_at_position(
        SettingsTab::Mounts,
        false,
        area,
        1,
        6,
    ));
    assert!(!settings_trust_clickable_at_position(
        SettingsTab::Trust,
        true,
        area,
        1,
        6,
    ));
    assert!(!settings_trust_clickable_at_position(
        SettingsTab::Trust,
        false,
        area,
        80,
        6,
    ));
}

#[test]
fn settings_scroll_focus_plan_routes_by_tab_and_modal() {
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Mounts, false, true),
        SettingsScrollFocusPlan {
            mounts: true,
            env: false,
            auth: false,
            trust: false,
        }
    );
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Auth, false, true),
        SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: true,
            trust: false,
        }
    );
    assert_eq!(
        settings_scroll_focus_plan(SettingsTab::Trust, true, true),
        SettingsScrollFocusPlan {
            mounts: false,
            env: false,
            auth: false,
            trust: false,
        }
    );
}

#[test]
fn settings_modal_open_reports_any_modal_surface() {
    assert!(!settings_modal_open(false, false, false, false));
    assert!(settings_modal_open(true, false, false, false));
    assert!(settings_modal_open(false, true, false, false));
    assert!(settings_modal_open(false, false, true, false));
    assert!(settings_modal_open(false, false, false, true));
}

#[test]
fn settings_horizontal_scroll_plan_updates_and_clamps_offset() {
    assert_eq!(settings_horizontal_scroll_plan(0, 8, 10, 40), 8);
    assert_eq!(settings_horizontal_scroll_plan(8, -99, 10, 40), 0);
}

#[test]
fn settings_env_selection_plan_skips_spacers_and_updates_scroll() {
    let rows = [
        SettingsEnvRow::Key {
            scope: SettingsEnvScope::Global,
            key: "ALPHA".to_owned(),
        },
        SettingsEnvRow::SectionSpacer,
        SettingsEnvRow::GlobalAddSentinel,
    ];
    let plan = settings_env_selection_plan(0, &rows, 1, 0, 8, 0);
    assert_eq!(plan.selected, 2);
    assert!(plan.scroll_y > 0);
}

#[test]
fn settings_global_mounts_selection_plan_clamps_to_add_row() {
    let plan = settings_global_mounts_selection_plan(0, 2, 99, 0, 8, 0);
    assert_eq!(plan.selected, 2);
    assert!(plan.scroll_y > 0);
    assert_eq!(settings_global_mounts_selected_index(99, 2), 2);
    assert!(settings_global_mounts_add_row_selected(2, 2));
    assert!(!settings_global_mounts_add_row_selected(1, 2));
    assert_eq!(settings_global_mounts_added_index(3), 2);
    assert_eq!(settings_global_mounts_added_index(0), 0);
    assert_eq!(settings_auth_selected_index(99, 2), 1);
    assert_eq!(settings_auth_selected_index(99, 0), 0);
}

fn env_config() -> SettingsEnvConfig<&'static str> {
    SettingsEnvConfig {
        env: BTreeMap::from([("GLOBAL".to_owned(), "x")]),
        roles: BTreeMap::from([
            (
                "alpha".to_owned(),
                BTreeMap::from([("ROLE_A".to_owned(), "x"), ("ROLE_B".to_owned(), "x")]),
            ),
            ("empty".to_owned(), BTreeMap::new()),
        ]),
    }
}

#[test]
fn settings_env_flat_rows_include_expanded_role_entries() {
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&env_config(), &expanded);
    assert!(matches!(rows[0], SettingsEnvRow::Key { .. }));
    assert!(matches!(rows[1], SettingsEnvRow::SectionSpacer));
    assert!(matches!(rows[2], SettingsEnvRow::GlobalAddSentinel));
    assert!(rows.iter().any(
        |row| matches!(row, SettingsEnvRow::RoleHeader { role, expanded: true } if role == "alpha")
    ));
    assert!(
        rows.iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
    );
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleHeader { role, .. } if role == "empty"))
    );
}

#[test]
fn settings_env_flat_rows_collapse_role_entries() {
    let rows = settings_env_flat_rows(&env_config(), &BTreeSet::new());
    assert!(rows.iter().any(
        |row| matches!(row, SettingsEnvRow::RoleHeader { role, expanded: false } if role == "alpha")
    ));
    assert!(
        !rows
            .iter()
            .any(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
    );
}

#[test]
fn settings_env_value_and_forbidden_keys_follow_scope() {
    let pending = env_config();

    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Global, "GLOBAL"),
        Some(&"x")
    );
    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Role("alpha".into()), "ROLE_A"),
        Some(&"x")
    );
    assert_eq!(
        forbidden_settings_env_keys(&pending, &SettingsEnvScope::Role("alpha".into())),
        vec!["ROLE_A".to_owned(), "ROLE_B".to_owned()]
    );
}

#[test]
fn settings_env_selected_key_matches_checks_selected_key_value() {
    let pending = env_config();
    let rows = settings_env_flat_rows(&pending, &BTreeSet::from(["alpha".to_owned()]));
    let role_b = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_B"
            )
        })
        .unwrap();

    assert!(settings_env_selected_key_matches(
        &pending,
        &rows,
        role_b,
        |value| *value == "x"
    ));
    assert!(!settings_env_selected_key_matches(
        &pending,
        &rows,
        role_b,
        |value| *value == "missing"
    ));
    assert!(!settings_env_selected_key_matches(
        &pending,
        &rows,
        usize::MAX,
        |_| true
    ));
}

#[test]
fn settings_env_selected_key_is_op_ref_checks_selected_value_shape() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([(
            "GLOBAL".to_owned(),
            EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/password".to_owned(),
                path: "Vault/Item/password".to_owned(),
                account: None,
                on_demand: false,
            }),
        )]),
        roles: BTreeMap::new(),
    };
    let rows = settings_env_flat_rows(&pending, &BTreeSet::new());

    assert!(settings_env_selected_key_is_op_ref(&pending, &rows, 0));
    assert!(!settings_env_selected_key_is_op_ref(
        &pending,
        &rows,
        usize::MAX,
    ));
}

#[test]
fn settings_env_selected_is_op_ref_builds_current_rows() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([(
            "GLOBAL".to_owned(),
            EnvValue::OpRef(jackin_core::OpRef {
                op: "op://vault/item/password".to_owned(),
                path: "Vault/Item/password".to_owned(),
                account: None,
                on_demand: false,
            }),
        )]),
        roles: BTreeMap::new(),
    };

    assert!(settings_env_selected_is_op_ref(
        &pending,
        &BTreeSet::new(),
        0
    ));
    assert!(!settings_env_selected_is_op_ref(
        &pending,
        &BTreeSet::new(),
        usize::MAX,
    ));
}

#[test]
fn settings_env_delete_key_for_row_extracts_key_rows_only() {
    let key_row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "TOKEN".to_owned(),
    };
    let header = SettingsEnvRow::RoleHeader {
        role: "ops".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_delete_key_for_row(Some(&key_row)),
        Some("TOKEN")
    );
    assert_eq!(settings_env_delete_key_for_row(Some(&header)), None);
    assert_eq!(settings_env_delete_key_for_row(None), None);
}

#[test]
fn settings_env_selected_delete_key_extracts_current_selected_key() {
    let pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_B"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_delete_key(&pending, &expanded, selected),
        Some("ROLE_B".to_owned())
    );
}

#[test]
fn set_settings_env_value_expands_role_scope() {
    let mut pending = SettingsEnvConfig {
        env: BTreeMap::new(),
        roles: BTreeMap::new(),
    };
    let mut expanded = BTreeSet::new();

    set_settings_env_value(
        &mut pending,
        &mut expanded,
        &SettingsEnvScope::Role("alpha".into()),
        "TOKEN",
        "secret",
    );

    assert_eq!(
        settings_env_value(&pending, &SettingsEnvScope::Role("alpha".into()), "TOKEN"),
        Some(&"secret")
    );
    assert!(expanded.contains("alpha"));
}

#[test]
fn toggle_settings_env_mask_for_row_skips_unmaskable_values() {
    let pending = env_config();
    let mut unmasked = BTreeSet::new();
    let row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "GLOBAL".to_owned(),
    };

    assert!(!toggle_settings_env_mask_for_row(
        &mut unmasked,
        &pending,
        Some(&row),
        |_| false
    ));
    assert!(unmasked.is_empty());

    assert!(toggle_settings_env_mask_for_row(
        &mut unmasked,
        &pending,
        Some(&row),
        |_| true
    ));
    assert!(unmasked.contains(&(SettingsEnvScope::Global, "GLOBAL".to_owned())));
}

#[test]
fn toggle_selected_settings_env_mask_uses_current_flat_selection() {
    let pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_A"
            )
        })
        .unwrap_or(usize::MAX);
    let mut unmasked = BTreeSet::new();

    assert!(toggle_selected_settings_env_mask(
        &mut unmasked,
        &pending,
        &expanded,
        selected,
        |_| true
    ));

    assert!(unmasked.contains(&(
        SettingsEnvScope::Role("alpha".to_owned()),
        "ROLE_A".to_owned()
    )));
}

#[test]
fn toggle_selected_settings_env_maskable_value_skips_op_refs() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([
            ("PLAIN".to_owned(), EnvValue::Plain("value".to_owned())),
            (
                "SECRET".to_owned(),
                EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/password".to_owned(),
                    path: "Vault/Item/password".to_owned(),
                    account: None,
                    on_demand: false,
                }),
            ),
        ]),
        roles: BTreeMap::new(),
    };
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let plain = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "PLAIN"))
        .unwrap_or(usize::MAX);
    let secret = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "SECRET"))
        .unwrap_or(usize::MAX);
    let mut unmasked = BTreeSet::new();

    assert!(toggle_selected_settings_env_maskable_value(
        &mut unmasked,
        &pending,
        &expanded,
        plain,
    ));
    assert!(!toggle_selected_settings_env_maskable_value(
        &mut unmasked,
        &pending,
        &expanded,
        secret,
    ));
}

#[test]
fn remove_settings_env_row_deletes_key_and_clamps_selection() {
    let mut pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let mut selected = 99;
    let row = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "ROLE_B".to_owned(),
    };

    assert!(remove_settings_env_row(
        &mut pending,
        &expanded,
        &mut selected,
        Some(&row),
    ));

    assert!(!pending.roles["alpha"].contains_key("ROLE_B"));
    assert_eq!(
        selected,
        settings_env_flat_row_count(&pending, &expanded) - 1
    );
}

#[test]
fn remove_selected_settings_env_row_uses_current_flat_selection() {
    let mut pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let mut selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_A"
            )
        })
        .unwrap_or(usize::MAX);

    assert!(remove_selected_settings_env_row(
        &mut pending,
        &expanded,
        &mut selected,
    ));

    assert!(!pending.roles["alpha"].contains_key("ROLE_A"));
    assert!(selected < settings_env_flat_row_count(&pending, &expanded));
}

#[test]
fn settings_env_add_target_follows_row_scope() {
    let global = SettingsEnvRow::GlobalAddSentinel;
    let role = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "TOKEN".to_owned(),
    };

    assert_eq!(
        settings_env_add_target_for_row(Some(&global)),
        Some(SettingsEnvScope::Global)
    );
    assert_eq!(
        settings_env_add_target_for_row(Some(&role)),
        Some(SettingsEnvScope::Role("alpha".to_owned()))
    );
}

#[test]
fn settings_env_selected_add_target_uses_current_flat_selection() {
    let pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::RoleAddSentinel(role) if role == "alpha"))
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_add_target(&pending, &expanded, selected),
        Some(SettingsEnvScope::Role("alpha".to_owned()))
    );
}

#[test]
fn settings_env_picker_target_skips_headers_and_spacers() {
    let key = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Role("alpha".to_owned()),
        key: "TOKEN".to_owned(),
    };
    let header = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_picker_target_for_row(Some(&key)),
        Some((
            SettingsEnvScope::Role("alpha".to_owned()),
            Some("TOKEN".to_owned())
        ))
    );
    assert_eq!(settings_env_picker_target_for_row(Some(&header)), None);
    assert_eq!(
        settings_env_picker_target_for_row(Some(&SettingsEnvRow::GlobalAddSentinel)),
        Some((SettingsEnvScope::Global, None))
    );
}

#[test]
fn settings_env_selected_picker_target_uses_current_flat_selection() {
    let pending = env_config();
    let expanded = BTreeSet::from(["alpha".to_owned()]);
    let rows = settings_env_flat_rows(&pending, &expanded);
    let selected = rows
        .iter()
        .position(|row| {
            matches!(
                row,
                SettingsEnvRow::Key {
                    scope: SettingsEnvScope::Role(role),
                    key,
                } if role == "alpha" && key == "ROLE_A"
            )
        })
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_picker_target(&pending, &expanded, selected),
        Some((
            SettingsEnvScope::Role("alpha".to_owned()),
            Some("ROLE_A".to_owned())
        ))
    );
}

#[test]
fn settings_env_enter_plan_handles_value_scope_and_headers() {
    let pending = env_config();
    let key = SettingsEnvRow::Key {
        scope: SettingsEnvScope::Global,
        key: "GLOBAL".to_owned(),
    };
    let collapsed = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: false,
    };
    let expanded = SettingsEnvRow::RoleHeader {
        role: "alpha".to_owned(),
        expanded: true,
    };

    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&key), |value| value.is_some()),
        SettingsEnvEnterPlan::EditValue {
            scope: SettingsEnvScope::Global,
            key: "GLOBAL".to_owned()
        }
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&key), |_| false),
        SettingsEnvEnterPlan::Noop
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&collapsed), |_| true),
        SettingsEnvEnterPlan::ExpandRole("alpha".to_owned())
    );
    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&expanded), |_| true),
        SettingsEnvEnterPlan::Noop
    );
}

#[test]
fn settings_env_enter_plan_handles_add_rows() {
    let pending = env_config();

    assert_eq!(
        settings_env_enter_plan_for_row(&pending, Some(&SettingsEnvRow::GlobalAddSentinel), |_| {
            true
        }),
        SettingsEnvEnterPlan::OpenScopePicker
    );
    assert_eq!(
        settings_env_enter_plan_for_row(
            &pending,
            Some(&SettingsEnvRow::RoleAddSentinel("alpha".to_owned())),
            |_| true
        ),
        SettingsEnvEnterPlan::AddRoleKey {
            scope: SettingsEnvScope::Role("alpha".to_owned()),
        }
    );
}

#[test]
fn settings_env_selected_enter_plan_skips_op_ref_values() {
    let pending = SettingsEnvConfig {
        env: BTreeMap::from([
            ("PLAIN".to_owned(), EnvValue::Plain("value".to_owned())),
            (
                "SECRET".to_owned(),
                EnvValue::OpRef(jackin_core::OpRef {
                    op: "op://vault/item/password".to_owned(),
                    path: "Vault/Item/password".to_owned(),
                    account: None,
                    on_demand: false,
                }),
            ),
        ]),
        roles: BTreeMap::new(),
    };
    let expanded = BTreeSet::new();
    let rows = settings_env_flat_rows(&pending, &expanded);
    let plain = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "PLAIN"))
        .unwrap_or(usize::MAX);
    let secret = rows
        .iter()
        .position(|row| matches!(row, SettingsEnvRow::Key { key, .. } if key == "SECRET"))
        .unwrap_or(usize::MAX);

    assert_eq!(
        settings_env_selected_enter_plan(&pending, &expanded, plain),
        SettingsEnvEnterPlan::EditValue {
            scope: SettingsEnvScope::Global,
            key: "PLAIN".to_owned()
        }
    );
    assert_eq!(
        settings_env_selected_enter_plan(&pending, &expanded, secret),
        SettingsEnvEnterPlan::Noop
    );
}

#[test]
fn settings_confirm_plan_routes_confirm_cancel_and_continue() {
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Save, ModalOutcome::Commit(true)),
        SettingsConfirmPlan::Commit
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Save, ModalOutcome::Commit(false)),
        SettingsConfirmPlan::Cancel {
            abort_sensitive: false
        }
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Sensitive, ModalOutcome::Cancel),
        SettingsConfirmPlan::Cancel {
            abort_sensitive: true
        }
    );
    assert_eq!(
        settings_confirm_plan(GlobalMountConfirm::Remove, ModalOutcome::Continue),
        SettingsConfirmPlan::Continue
    );
}

#[test]
fn settings_confirm_commit_plan_routes_confirmed_actions() {
    assert_eq!(
        settings_confirm_commit_plan(GlobalMountConfirm::Remove, 1, 3),
        SettingsConfirmCommitPlan::Remove {
            remove_index: 1,
            selected: 1,
        }
    );
    assert_eq!(
        settings_confirm_commit_plan(GlobalMountConfirm::Remove, 2, 3),
        SettingsConfirmCommitPlan::Remove {
            remove_index: 2,
            selected: 2,
        }
    );
    assert_eq!(
        settings_confirm_commit_plan(GlobalMountConfirm::Remove, 9, 3),
        SettingsConfirmCommitPlan::Noop
    );
    assert_eq!(
        settings_confirm_commit_plan(GlobalMountConfirm::Save, 0, 0),
        SettingsConfirmCommitPlan::Save
    );
    assert_eq!(
        settings_confirm_commit_plan(GlobalMountConfirm::Sensitive, 0, 0),
        SettingsConfirmCommitPlan::OpenSavePreview
    );
    assert_eq!(
        settings_confirm_commit_plan(GlobalMountConfirm::Discard, 0, 0),
        SettingsConfirmCommitPlan::DiscardAll
    );
}
