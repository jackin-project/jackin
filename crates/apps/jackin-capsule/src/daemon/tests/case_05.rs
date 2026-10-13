// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn apply_dialog_action_switch_usage_provider_resolves_exact_account_id() {
    use jackin_protocol::control::{
        FocusedAccountHeader, FocusedUsageView, UsageConfidence, UsageSnapshotStatus, UsageSource,
    };
    use jackin_protocol::usage_broker::UsageAccountCapability;

    fn account_view(account: &str, fetched_at: i64) -> FocusedUsageView {
        let mut view = FocusedUsageView::unavailable("none", fetched_at);
        view.account = FocusedAccountHeader {
            provider_label: "Anthropic".to_owned(),
            account_label: account.to_owned(),
            username: None,
            plan_label: None,
            credential_origin: None,
        };
        view.status = UsageSnapshotStatus::Fresh;
        view.source = UsageSource::ProviderApi;
        view.confidence = UsageConfidence::Authoritative;
        view
    }

    let mut mux = single_pane_tab_mux();
    for (id, broker_id, account) in [
        (1_u64, "test-claude-a", "a@example.com"),
        (2, "test-claude-b", "b@example.com"),
    ] {
        let (mut session, _rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
        session.provider = Some(crate::session::SessionProvider {
            label: "Anthropic".to_owned(),
            env_overrides: Vec::new(),
        });
        session.usage_capability = Some(UsageAccountCapability {
            account_id: broker_id.to_owned(),
            surface_id: "claude".to_owned(),
        });
        mux.session_supervisor.sessions.insert(id, session);
        mux.usage
            .usage_cache
            .insert_snapshot_for_capability_for_test(
                "claude",
                Some("Anthropic"),
                &UsageAccountCapability {
                    account_id: broker_id.to_owned(),
                    surface_id: "claude".to_owned(),
                },
                account_view(account, 100 + id.cast_signed()),
            );
    }
    mux.session_supervisor.tabs[0] = Tab::new_single("Claude", 1, "test");
    mux.dialog_push(Dialog::new_usage(FocusedUsageView::unavailable("seed", 1)));

    // Switch to the second same-provider account by exact id: the dialog
    // focuses that account and the queued refresh targets its capability,
    // even though both tabs share the provider head.
    let id_b = jackin_core::account_key_hash("Anthropic", "b@example.com");
    mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
        provider_label: "Anthropic · b@example.com".to_owned(),
        account_id: id_b.clone(),
    });
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("switch usage provider action must keep usage dialog open");
    };
    assert_eq!(view.account.account_label, "b@example.com");
    assert_eq!(view.tabs.len(), 2);
    assert_ne!(view.tabs[0].label, view.tabs[1].label);
    let active: Vec<_> = view.tabs.iter().filter(|tab| tab.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id_b);
    assert_eq!(
        mux.usage.pending_usage_refresh,
        Some(crate::usage::UsageRefreshTarget {
            agent: "claude".to_owned(),
            provider: Some("Anthropic".to_owned()),
            capability: UsageAccountCapability {
                account_id: "test-claude-b".to_owned(),
                surface_id: "claude".to_owned(),
            },
        })
    );

    // Unknown id: honest unavailable, and the queued refresh is untouched
    // rather than overwritten with a label-guessed sibling target.
    mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
        provider_label: "Anthropic · b@example.com".to_owned(),
        account_id: "sha256:unknown".to_owned(),
    });
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("switch usage provider action must keep usage dialog open");
    };
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(
        view.account.account_label,
        "usage unavailable: account not cached"
    );
    assert_eq!(
        mux.usage
            .pending_usage_refresh
            .as_ref()
            .expect("queued refresh untouched")
            .capability
            .account_id,
        "test-claude-b"
    );
}

#[test]
fn apply_action_open_usage_queues_focused_provider_refresh() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor.tabs[0] = Tab::new_single("Codex", 1, "test");

    mux.apply_action(Action::OpenUsage);

    assert!(matches!(mux.dialog_top(), Some(Dialog::Usage { .. })));
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog open") else {
        panic!("usage dialog expected");
    };
    // Bug 1: the action only QUEUES the refresh (pending_usage_refresh set
    // below); the "refreshing" marker is applied by the dialog tick only when a
    // refresh task is genuinely in flight. No task is spawned here, so the marker
    // must NOT appear — it is no longer driven by the scheduling flag.
    assert!(
        !view.updated_label.contains("refreshing"),
        "{:?}",
        view.updated_label
    );
    assert_eq!(
        mux.usage.pending_usage_refresh,
        Some(crate::usage::UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
            capability: jackin_protocol::usage_broker::UsageAccountCapability {
                account_id: "test-codex".to_owned(),
                surface_id: "codex".to_owned(),
            },
        })
    );
}

#[test]
fn open_usage_dialog_refreshes_visible_relative_timestamp_from_cache() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    let capability = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "test-codex".to_owned(),
        surface_id: "codex".to_owned(),
    };
    session.usage_capability = Some(capability.clone());
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor.tabs[0] = Tab::new_single("Codex", 1, "test");
    let now_epoch = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time after epoch")
            .as_secs(),
    )
    .unwrap_or(i64::MAX);
    let cached = jackin_protocol::control::FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: jackin_protocol::control::FocusedAccountHeader {
            provider_label: "Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
        },
        buckets: vec![jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Session".to_owned(),
            used_label: Some("63% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(37),
            reset_label: Some("Resets at 15:00 UTC".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        }],
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
        source: jackin_protocol::control::UsageSource::Cli,
        confidence: jackin_protocol::control::UsageConfidence::Authoritative,
        fetched_at_epoch: now_epoch - 120,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    };
    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test("codex", Some("OpenAI"), &capability, cached);
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable("seed", 1);
    view.updated_label = "Updated just now".to_owned();
    mux.dialog_push(Dialog::new_usage(view));

    assert!(mux.refresh_open_usage_dialog_from_cache());

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog open") else {
        panic!("usage dialog expected");
    };
    assert_eq!(view.updated_label, "Updated 2m ago");
}

#[test]
fn outer_terminal_title_uses_workspace_and_pr_title() {
    let title = compose_outer_terminal_title(
        Path::new("/Users/operator/Projects/jackin"),
        Some("feat/capsule-pr-context-bar"),
        Some(&pull_request_fixture(436)),
    );

    assert_eq!(title, "jackin · PR #436 · Surface PR context in Capsule");
}

#[test]
fn outer_terminal_title_falls_back_to_branch_without_pr() {
    let title = compose_outer_terminal_title(
        Path::new("/Users/operator/Projects/jackin"),
        Some("feat/capsule-pr-context-bar"),
        None,
    );

    assert_eq!(title, "jackin · feat/capsule-pr-context-bar");
}

#[test]
fn outer_terminal_title_sanitizes_control_bytes() {
    let pull_request = PullRequestInfo {
        number: 436,
        title: "bad\x1b]2;owned\x07title".to_owned(),
        url: "https://github.com/jackin-project/jackin/pull/436".to_owned(),
        is_draft: false,
        checks: None,
    };
    let title =
        compose_outer_terminal_title(Path::new("/workspace/jackin"), None, Some(&pull_request));

    assert_eq!(title, "jackin · PR #436 · bad ]2;owned title");
}

#[test]
fn display_title_falls_back_when_shell_sets_empty_title() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]2;\x07");

    assert_eq!(session_display_title(&session), "Test");
}

#[test]
fn display_title_uses_shell_title_without_repeating_shell_label() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]2;prompt title\x07");

    assert_eq!(session_display_title(&session), "prompt title");
}

#[test]
fn display_title_uses_shell_cwd_without_repeating_shell_label() {
    let (mut session, _rx) = test_shell_session(20, 80);
    session.feed_pty(b"\x1b]7;file:///workspace/project\x07");

    assert_eq!(session_display_title(&session), "/workspace/project");
}

#[test]
fn full_frame_emits_outer_terminal_title_once_until_context_changes() {
    let mut mux = single_pane_tab_mux();
    mux.launch_env.workdir = PathBuf::from("/workspace/jackin");
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/capsule-pr-context-bar"));

    let first = String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
        .to_string();
    assert!(
        first.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
        "first frame should set branch title: {first:?}"
    );

    let second =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        !second.contains("\x1b]2;jackin · feat/capsule-pr-context-bar\x1b\\"),
        "unchanged full frame should not spam title: {second:?}"
    );

    mux.pr_watch.pull_request_context = Some(Arc::new(pull_request_fixture(436)));
    let updated =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        updated.contains("\x1b]2;jackin · PR #436 · Surface PR context in Capsule\x1b\\"),
        "PR context change should refresh title: {updated:?}"
    );
}

#[test]
fn full_frame_updates_outer_terminal_title_on_branch_switch() {
    let mut mux = single_pane_tab_mux();
    mux.launch_env.workdir = PathBuf::from("/workspace/jackin");
    mux.launch_env.workdir_context.default_branch = Some("main".to_owned());
    mux.pr_watch.pull_request_context_branch = Some(branch("feat/a"));

    let first = String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
        .to_string();
    assert!(
        first.contains("\x1b]2;jackin · feat/a\x1b\\"),
        "first non-default branch should set title: {first:?}"
    );

    mux.pr_watch.pull_request_context_branch = Some(branch("feat/b"));
    let switched =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        switched.contains("\x1b]2;jackin · feat/b\x1b\\"),
        "branch switch should refresh title: {switched:?}"
    );

    mux.pr_watch.pull_request_context_branch = Some(branch("main"));
    let default_branch =
        String::from_utf8_lossy(&compose_after(&mut mux, FullRedrawReason::ExplicitRedraw))
            .to_string();
    assert!(
        default_branch.contains("\x1b]2;jackin\x1b\\"),
        "default branch should fall back to workspace-only title: {default_branch:?}"
    );
    assert!(
        !default_branch.contains("jackin · main"),
        "default branch name should not be propagated into title: {default_branch:?}"
    );
}

#[test]
fn refresh_tab_labels_preserves_provider_suffix() {
    let mut mux = test_mux(24, 80);
    let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("Claude", 1, "test"));

    mux.refresh_tab_labels();

    assert_eq!(mux.session_supervisor.tabs[0].label(), "Claude (Z.AI)");
}

#[test]
fn split_metadata_inherits_focused_provider() {
    let mut mux = test_mux(24, 80);
    let (session, _rx) = test_provider_session(jackin_protocol::Provider::Zai);
    let expected_env = session
        .provider
        .as_ref()
        .map(|p| p.env_overrides.clone())
        .unwrap_or_default();
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor
        .tabs
        .push(Tab::new_single("Claude (Z.AI)", 1, "test"));

    let (agent, env, provider) = mux.focused_spawn_metadata();

    assert_eq!(agent.as_deref(), Some("claude"));
    assert_eq!(provider.as_deref(), Some("Z.AI"));
    assert_eq!(env, expected_env);
}
