// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[tokio::test]
async fn unauthorized_session_control_returns_unknown_without_sibling_input() {
    let mut mux = single_pane_tab_mux();
    let own = jackin_protocol::SessionIdentity {
        uid: 2_111,
        gid: 2_111,
    };
    let sibling = jackin_protocol::SessionIdentity {
        uid: 2_112,
        gid: 2_112,
    };
    mux.launch_env.launch_config.instance_identities =
        BTreeMap::from([("own".to_owned(), own), ("sibling".to_owned(), sibling)]);
    let (mut own_session, _own_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    own_session.identity = own;
    let (mut sibling_session, mut sibling_rx) =
        test_session_with_agent(24, 80, Some("claude".to_owned()));
    sibling_session.identity = sibling;
    mux.session_supervisor.sessions.insert(1, own_session);
    mux.session_supervisor.sessions.insert(2, sibling_session);

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle_control_request(
        &mut mux,
        ControlRequest {
            ctx: jackin_protocol::TelemetryContext::v1(),
            session_capability: None,
            peer_uid: own.uid,
            msg: ClientMsg::SessionSend {
                session: 2,
                text: "must-not-reach-sibling".to_owned(),
            },
            reply: crate::attach_protocol::ControlReply::Once(reply_tx),
        },
    );

    let response = reply_rx.await.expect("authorization response");
    assert!(matches!(response.msg, ServerMsg::Unknown));
    assert!(
        sibling_rx.try_recv().is_err(),
        "unauthorized sibling input must not reach its PTY"
    );
}

#[tokio::test]
async fn unauthorized_exec_is_rejected_before_picker_mutation() {
    let mut mux = single_pane_tab_mux();
    let identity = jackin_protocol::SessionIdentity {
        uid: 2_121,
        gid: 2_121,
    };
    mux.launch_env.launch_config.instance_identities =
        BTreeMap::from([("agent".to_owned(), identity)]);
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.identity = identity;
    mux.session_supervisor.sessions.insert(1, session);
    let initial_dialog_count = mux.control.dialog_stack.len();

    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
    handle_control_request(
        &mut mux,
        ControlRequest {
            ctx: jackin_protocol::TelemetryContext::v1(),
            session_capability: None,
            peer_uid: identity.uid,
            msg: ClientMsg::ExecCommand {
                command: "gh".to_owned(),
                args: vec!["auth".to_owned(), "status".to_owned()],
            },
            reply: crate::attach_protocol::ControlReply::Once(reply_tx),
        },
    );

    let response = reply_rx.await.expect("authorization response");
    assert!(matches!(response.msg, ServerMsg::Unknown));
    assert_eq!(mux.control.dialog_stack.len(), initial_dialog_count);
    assert!(
        mux.control.pending_exec_reply.is_none(),
        "unauthorized ExecCommand must not retain a deferred picker reply"
    );
}

#[test]
fn control_reply_for_request_shapes_usage_variants() {
    let mut mux = single_pane_tab_mux();
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: "OpenAI".to_owned(),
        env_overrides: Vec::new(),
    });
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor.tabs[0] = Tab::new_single("Codex", 1, "test");
    let focused = control_reply_for_request(&mut mux, ClientMsg::UsageFocused);
    assert!(matches!(focused, ServerMsg::UsageFocused { .. }));

    let refreshed = control_reply_for_request(&mut mux, ClientMsg::UsageRefreshFocused);
    assert!(matches!(refreshed, ServerMsg::UsageFocused { .. }));
    assert!(
        mux.usage.pending_usage_refresh.is_some(),
        "refresh request should queue provider work instead of probing inline"
    );

    let accounts = control_reply_for_request(&mut mux, ClientMsg::UsageAccountList);
    assert!(matches!(accounts, ServerMsg::UsageAccounts { .. }));
}

#[test]
fn control_reply_exposes_typed_telemetry_health() {
    const CHILD: &str = "JACKIN_CAPSULE_TELEMETRY_HEALTH_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = Command::new(std::env::current_exe().expect("current test executable"))
            .args([
                "--exact",
                "daemon::tests::case_04::control_reply_exposes_typed_telemetry_health",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .status()
            .expect("spawn isolated telemetry health test");
        assert!(status.success(), "isolated telemetry health test failed");
        return;
    }
    let _telemetry_guard = crate::support::telemetry_test_guard();
    let mut mux = single_pane_tab_mux();
    let reply = control_reply_for_request(&mut mux, ClientMsg::TelemetryHealth);
    let ServerMsg::TelemetryHealth { report } = reply else {
        panic!("expected telemetry health");
    };
    assert_eq!(report.fingerprint.service_name, "jackin-capsule");
    assert_eq!(report.fingerprint.app_mode, "capsule");
    assert_eq!(report.fingerprint.compression, "gzip");
    assert_eq!(report.fingerprint.sampler, "parentbased_always_on");
    assert_eq!(report.config_failure, None);
    assert!(report.health.active_signals <= 3);
    assert_eq!(
        report.health.capsule_export,
        jackin_protocol::control::CapsuleExportCoverage::NotApplicable
    );
    assert_eq!(
        report.health.flush,
        jackin_protocol::control::TelemetryFlushStatus::Pending
    );
    assert!(!report.health.shutdown_timed_out);
    let json = serde_json::to_string(&report).unwrap().to_ascii_lowercase();
    assert!(!json.contains("authorization"));
    assert!(!json.contains("header"));
    assert!(!json.contains("certificate"));
}

#[test]
fn control_reply_report_runtime_event_applies_to_session_and_acks() {
    let mut mux = single_pane_tab_mux();
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("opencode".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);

    let reply = control_reply_for_request(
        &mut mux,
        ClientMsg::ReportRuntimeEvent {
            session_id: 1,
            source_id: "hook-opencode-1".to_owned(),
            runtime: "opencode".to_owned(),
            event: "permission.asked".to_owned(),
            payload: None,
        },
    );

    assert!(matches!(reply, ServerMsg::Ack));
    let authority = mux
        .session_supervisor
        .sessions
        .get(1)
        .expect("session is registered")
        .authority
        .as_ref()
        .expect("event applied to the addressed session's authority");
    assert_eq!(authority.source_id, "hook-opencode-1");
    assert!(authority.pending_permission);
}

#[test]
fn control_reply_runtime_event_and_capture_for_unknown_session_still_ack() {
    // The hook must never be blocked or failed by a stale/wrong session id: both
    // control messages Ack (and do not panic) when the session is absent.
    let mut mux = single_pane_tab_mux();

    let event_reply = control_reply_for_request(
        &mut mux,
        ClientMsg::ReportRuntimeEvent {
            session_id: 999,
            source_id: "hook-opencode-1".to_owned(),
            runtime: "opencode".to_owned(),
            event: "permission.asked".to_owned(),
            payload: None,
        },
    );
    assert!(matches!(event_reply, ServerMsg::Ack));

    let capture_reply =
        control_reply_for_request(&mut mux, ClientMsg::StatusCapture { session_id: 999 });
    assert!(matches!(capture_reply, ServerMsg::Ack));
}

#[test]
fn control_usage_account_list_uses_in_memory_cache() {
    let mut mux = single_pane_tab_mux();
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable("seed", 123);
    view.focused_agent = Some("codex".to_owned());
    view.focused_provider = Some("OpenAI".to_owned());
    view.account = jackin_protocol::control::FocusedAccountHeader {
        provider_label: "OpenAI / Codex".to_owned(),
        account_label: "codex@example.com".to_owned(),
        username: None,
        plan_label: Some("Pro 20x".to_owned()),
        credential_origin: None,
    };
    view.status = jackin_protocol::control::UsageSnapshotStatus::Fresh;
    view.source = jackin_protocol::control::UsageSource::ProviderApi;
    view.confidence = jackin_protocol::control::UsageConfidence::Authoritative;
    view.buckets = vec![jackin_protocol::control::QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::default(),
        label: "Session".to_owned(),
        used_label: Some("63% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(37),
        reset_label: Some("Resets in 2h".to_owned()),
        resets_at: None,
        status_slot: None,
        pace_label: None,
        status: jackin_protocol::control::UsageSnapshotStatus::Fresh,
    }];
    mux.usage
        .usage_cache
        .insert_snapshot_for_test("codex", Some("OpenAI"), view);

    let accounts = control_reply_for_request(&mut mux, ClientMsg::UsageAccountList);

    let ServerMsg::UsageAccounts { accounts } = accounts else {
        panic!("usage accounts response expected");
    };
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].provider, "OpenAI / Codex");
    assert_eq!(accounts[0].account_label, "codex@example.com");
    assert_eq!(accounts[0].used_amount, Some(63));
}

#[test]
fn apply_dialog_action_refresh_usage_queues_refresh_without_replacing_dialog() {
    let mut mux = single_pane_tab_mux();
    seed_usage_dialog_for_refresh_test(&mut mux);

    mux.apply_dialog_action(DialogAction::RefreshUsage);

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("refresh usage action must keep usage dialog open");
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
    assert_eq!(view.status_bar_label, "seed");
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
fn apply_action_refresh_usage_queues_refresh_without_replacing_dialog() {
    let mut mux = single_pane_tab_mux();
    seed_usage_dialog_for_refresh_test(&mut mux);

    mux.apply_action(Action::RefreshUsage);

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("refresh usage action must keep usage dialog open");
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
    assert_eq!(view.status_bar_label, "seed");
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
fn apply_dialog_action_switch_usage_provider_updates_focused_provider() {
    let mut mux = single_pane_tab_mux();
    let (session, _session_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
    mux.session_supervisor.sessions.insert(1, session);
    mux.session_supervisor.tabs[0] = Tab::new_single("Codex", 1, "test");
    mux.dialog_push(Dialog::new_usage(
        jackin_protocol::control::FocusedUsageView {
            focused_provider: Some("MiniMax".to_owned()),
            account: jackin_protocol::control::FocusedAccountHeader {
                provider_label: "Usage".to_owned(),
                account_label: "seed".to_owned(),
                username: None,
                plan_label: None,
                credential_origin: None,
            },
            ..jackin_protocol::control::FocusedUsageView::unavailable("seed", 1)
        },
    ));

    mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
        provider_label: "Claude".to_owned(),
        // Empty id: old payloads keep label resolution.
        account_id: String::new(),
    });

    let Dialog::Usage { view, .. } = mux.dialog_top().expect("usage dialog still open") else {
        panic!("switch usage provider action must keep usage dialog open");
    };
    assert_eq!(view.focused_provider.as_deref(), Some("Claude"));
    assert_eq!(view.account.provider_label, "Anthropic");
    assert_eq!(
        mux.usage.pending_usage_refresh,
        Some(crate::usage::UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("Claude".to_owned()),
            capability: jackin_protocol::usage_broker::UsageAccountCapability {
                account_id: "test-codex".to_owned(),
                surface_id: "codex".to_owned(),
            },
        })
    );
}
