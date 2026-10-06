// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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

#[test]
fn settings_auth_row_kind_addresses_accounts_add_github_and_scan() {
    assert_eq!(
        settings_auth_row_kind(2, 0),
        SettingsAuthRowKind::Account(0)
    );
    assert_eq!(
        settings_auth_row_kind(2, 1),
        SettingsAuthRowKind::Account(1)
    );
    for (offset, kind) in ACCOUNT_KINDS.iter().enumerate() {
        assert_eq!(
            settings_auth_row_kind(2, 2 + offset),
            SettingsAuthRowKind::AddKind(*kind)
        );
    }
    let github = 2 + ACCOUNT_KINDS.len();
    assert_eq!(
        settings_auth_row_kind(2, github),
        SettingsAuthRowKind::Github
    );
    assert_eq!(
        settings_auth_row_kind(2, github + 1),
        SettingsAuthRowKind::Scan
    );
    assert!(!settings_auth_scan_row_selected(2, github));
    assert!(settings_auth_scan_row_selected(2, github + 1));
}

#[test]
fn enter_selected_kind_leaves_scan_row_kindless() {
    let mut auth = scan_test_auth();
    auth.selected = auth.pending.len() + ACCOUNT_KINDS.len();
    auth.enter_selected_kind();
    assert_eq!(
        auth.selected_kind(),
        Some(crate::tui::auth::AuthKind::Github)
    );
    auth.selected = auth.row_count() - 1;
    auth.enter_selected_kind();
    assert_eq!(auth.selected_kind(), None);
}

#[test]
fn reduce_account_scan_message_arms_dedupes_and_ignores_foreign() {
    let mut auth = scan_test_auth();
    let effect = reduce_account_scan_message(&mut auth, &SettingsMessage::RequestAccountScan);
    assert_eq!(
        effect,
        Some(SettingsEffect::StartAccountScan { generation: 1 })
    );
    assert!(auth.scan.in_flight);
    // Concurrent UI scans dedupe while one is in flight.
    let duplicate = reduce_account_scan_message(&mut auth, &SettingsMessage::RequestAccountScan);
    assert_eq!(duplicate, None);
    // Non-scan messages pass through untouched.
    let foreign = reduce_account_scan_message(&mut auth, &SettingsMessage::FocusTabBar);
    assert_eq!(foreign, None);
    assert!(auth.scan.in_flight);

    assert_eq!(
        reduce_account_scan_message(&mut auth, &SettingsMessage::CancelAccountScan),
        None
    );
    assert!(!auth.scan.in_flight);
    // The orphaned completion (stale generation) is ignored.
    let outcome = model::AccountScanOutcome::default();
    assert_eq!(
        reduce_account_scan_message(
            &mut auth,
            &SettingsMessage::AccountScanCompleted {
                generation: 1,
                result: Ok(outcome),
            },
        ),
        None
    );
    assert!(!auth.pending.contains_key("bootstrap"));
    assert!(!auth.scan.in_flight);
}

#[test]
fn reduce_account_scan_completion_joins_candidates_and_committed() {
    use super::model::AccountScanOutcome;
    let mut auth = scan_test_auth();
    let effect = reduce_account_scan_message(&mut auth, &SettingsMessage::RequestAccountScan);
    assert_eq!(
        effect,
        Some(SettingsEffect::StartAccountScan { generation: 1 })
    );
    let outcome = AccountScanOutcome {
        fresh_install: true,
        committed: vec![(
            "bootstrap".to_owned(),
            scan_profile_account("/home/op/.claude"),
        )],
        candidates: vec![("cand".to_owned(), scan_key_account("CAND_KEY"))],
        issues: vec![jackin_config::DiscoveryIssue {
            agent: jackin_core::Agent::Codex,
            directory: "/home/op/.codex".into(),
            error: jackin_config::DiscoveryError::Unreadable,
        }],
    };
    let effect = reduce_account_scan_message(
        &mut auth,
        &SettingsMessage::AccountScanCompleted {
            generation: 1,
            result: Ok(outcome),
        },
    );
    assert_eq!(effect, None);
    assert!(!auth.scan.in_flight);
    // Committed refresh both layers; candidates join pending only.
    assert!(auth.pending.contains_key("bootstrap"));
    assert!(auth.original.contains_key("bootstrap"));
    assert!(auth.pending.contains_key("cand"));
    assert!(!auth.original.contains_key("cand"));
    assert!(auth.panel_is_dirty());
    assert!(auth.scan.scanned_ids.contains("bootstrap"));
    assert!(auth.scan.scanned_ids.contains("cand"));
    let summary = auth.scan.last_summary.as_ref().unwrap();
    assert_eq!(
        summary.joined,
        vec!["bootstrap".to_owned(), "cand".to_owned()]
    );
    assert!(summary.skipped.is_empty());
    assert!(summary.fresh_install);
    assert_eq!(auth.scan.issues.len(), 1);
}

#[test]
fn scan_merge_never_touches_present_or_deleted_ids() {
    use super::model::AccountScanOutcome;
    let mut auth = scan_test_auth();
    // Operator deleted "gone" from pending and hand-edited nothing else.
    auth.original
        .insert("gone".to_owned(), scan_key_account("GONE_KEY"));
    auth.pending.remove("gone");
    auth.pending
        .insert("mine".to_owned(), scan_key_account("MINE_KEY"));
    let before = auth.pending.get("keep").cloned().unwrap();

    let outcome = AccountScanOutcome {
        fresh_install: false,
        committed: vec![("keep".to_owned(), scan_profile_account("/elsewhere"))],
        candidates: vec![
            ("keep".to_owned(), scan_profile_account("/elsewhere")),
            ("gone".to_owned(), scan_key_account("GONE_KEY")),
            ("alias".to_owned(), scan_key_account("MINE_KEY")),
            ("fresh".to_owned(), scan_key_account("FRESH_KEY")),
        ],
        issues: Vec::new(),
    };
    let summary = auth.merge_account_scan_outcome(&outcome);
    assert_eq!(summary.joined, vec!["fresh".to_owned()]);
    assert_eq!(summary.skipped.len(), 4);
    // Present IDs keep their exact content; the deletion stands.
    assert_eq!(auth.pending.get("keep"), Some(&before));
    assert!(!auth.pending.contains_key("gone"));
    assert!(!auth.pending.contains_key("alias"));
    assert!(auth.pending.contains_key("fresh"));
}

#[test]
fn scan_merge_failure_surfaces_panel_error() {
    let mut auth = scan_test_auth();
    assert_eq!(
        reduce_account_scan_message(&mut auth, &SettingsMessage::RequestAccountScan),
        Some(SettingsEffect::StartAccountScan { generation: 1 })
    );
    assert_eq!(
        reduce_account_scan_message(
            &mut auth,
            &SettingsMessage::AccountScanCompleted {
                generation: 1,
                result: Err("lock poisoned".to_owned()),
            },
        ),
        None
    );
    assert!(!auth.scan.in_flight);
    assert_eq!(auth.take_error().as_deref(), Some("lock poisoned"));
    assert_eq!(auth.pending.len(), 1);
}

#[test]
fn discard_orphans_in_flight_scan() {
    let mut auth = scan_test_auth();
    assert_eq!(
        reduce_account_scan_message(&mut auth, &SettingsMessage::RequestAccountScan),
        Some(SettingsEffect::StartAccountScan { generation: 1 })
    );
    auth.panel_discard();
    assert!(!auth.scan.in_flight);
    assert!(auth.scan.scanned_ids.is_empty());
    // A stale completion after discard joins nothing.
    let outcome = model::AccountScanOutcome {
        candidates: vec![("cand".to_owned(), scan_key_account("CAND_KEY"))],
        ..model::AccountScanOutcome::default()
    };
    auth.complete_account_scan(1, &Ok(outcome));
    assert!(!auth.pending.contains_key("cand"));
}

#[test]
fn scanned_source_in_draft_mirrors_upsert_rule() {
    let pending = BTreeMap::from([
        (
            "profile".to_owned(),
            scan_profile_account("/home/op/.claude"),
        ),
        ("key".to_owned(), scan_key_account("MINE_KEY")),
    ]);
    // Same profile store: duplicate.
    assert!(scanned_source_in_draft(
        &pending,
        &scan_profile_account("/home/op/.claude")
    ));
    // Same agent, different store: distinct.
    assert!(!scanned_source_in_draft(
        &pending,
        &scan_profile_account("/home/op/.claude-work")
    ));
    // Same key reference: duplicate.
    assert!(scanned_source_in_draft(
        &pending,
        &scan_key_account("MINE_KEY")
    ));
    // Same reference, different endpoint: distinct (matches upsert).
    let mut forked = scan_key_account("MINE_KEY");
    if let jackin_config::AccountCredential::ApiKey { base_url, .. } = &mut forked.credential {
        *base_url = Some("https://proxy.example".into());
    }
    assert!(!scanned_source_in_draft(&pending, &forked));
    // Different provider, same value shape: distinct.
    let mut other_provider = scan_key_account("MINE_KEY");
    other_provider.provider = jackin_config::AiProvider::OpenAi;
    assert!(!scanned_source_in_draft(&pending, &other_provider));
}
