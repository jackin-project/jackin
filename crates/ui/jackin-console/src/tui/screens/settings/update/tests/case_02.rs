// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
