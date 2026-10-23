// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn apply_dialog_action_switch_usage_provider_resolves_bound_full_tuple() {
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
            account_capability: None,
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

    let capability_b = UsageAccountCapability {
        account_id: "test-claude-b".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let id_b = capability_b.opaque_identity_key();
    mux.launch_env.launch_config.usage_capabilities =
        BTreeMap::from([("claude".to_owned(), capability_b.clone())]);
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

struct BoundUsageFixture<'a> {
    capability: &'a jackin_protocol::usage_broker::UsageAccountCapability,
    agent: &'a str,
    provider: &'a str,
    account_label: &'a str,
    plan_label: Option<&'a str>,
    fetched_at: i64,
    status: jackin_protocol::control::UsageSnapshotStatus,
    source: jackin_protocol::control::UsageSource,
    status_bar_label: &'a str,
    last_error: Option<&'a str>,
}

fn bound_usage_view_for_test(
    fixture: BoundUsageFixture<'_>,
) -> jackin_protocol::control::FocusedUsageView {
    let BoundUsageFixture {
        capability,
        agent,
        provider,
        account_label,
        plan_label,
        fetched_at,
        status,
        source,
        status_bar_label,
        last_error,
    } = fixture;
    let mut view = jackin_protocol::control::FocusedUsageView::unavailable("fixture", fetched_at);
    view.focused_agent = Some(agent.to_owned());
    view.focused_provider = Some(provider.to_owned());
    view.account = jackin_protocol::control::FocusedAccountHeader {
        provider_label: provider.to_owned(),
        account_label: account_label.to_owned(),
        username: Some(format!("user-{account_label}")),
        plan_label: plan_label.map(str::to_owned),
        credential_origin: Some("synthetic test origin".to_owned()),
        account_capability: Some(capability.clone()),
    };
    view.status = status;
    view.source = source;
    view.confidence = jackin_protocol::control::UsageConfidence::Authoritative;
    view.fetched_at_epoch = fetched_at;
    view.updated_label = format!("{account_label} updated");
    view.status_bar_label = status_bar_label.to_owned();
    view.last_error = last_error.map(str::to_owned);
    view
}

fn install_usage_session_for_test(
    mux: &mut Multiplexer,
    session_id: u64,
    agent: &str,
    provider: &str,
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) {
    let (mut session, _session_rx) = test_session_with_agent(24, 80, Some(agent.to_owned()));
    session.provider = Some(crate::session::SessionProvider {
        label: provider.to_owned(),
        env_overrides: Vec::new(),
    });
    session.usage_capability = Some(capability.clone());
    mux.session_supervisor.sessions.insert(session_id, session);
}

fn account_a_view(
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> jackin_protocol::control::FocusedUsageView {
    use jackin_protocol::control::{
        CanonicalRemainingPercent, PercentObservationOrigin, QuotaBucketView, StatusSlot,
        UsageSnapshotStatus, UsageSource,
    };

    let mut view = bound_usage_view_for_test(BoundUsageFixture {
        capability,
        agent: "claude",
        provider: "Anthropic",
        account_label: "shared@example.test",
        plan_label: Some("A plan"),
        fetched_at: 1_000,
        status: UsageSnapshotStatus::Stale,
        source: UsageSource::Cache,
        status_bar_label: "A status owner",
        last_error: Some("A only error"),
    });
    view.buckets = vec![QuotaBucketView {
        label: "A-only quota".to_owned(),
        used_label: Some("A used".to_owned()),
        limit_label: Some("A limit".to_owned()),
        used_percent: None,
        used_percent_origin: None,
        remaining_percent: Some(CanonicalRemainingPercent::new(91.25).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        count_quota: None,
        provider_quota_state: None,
        reset_label: Some("A-only reset".to_owned()),
        resets_at: Some(10_000),
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Stale,
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::Normal,
    }];
    view
}

fn account_b_view(
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> jackin_protocol::control::FocusedUsageView {
    use jackin_protocol::control::{
        CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
        CountQuotaProvenance, CountQuotaUnit, Money, PercentObservationOrigin, ProviderQuotaState,
        QuotaBucketView, StatusSlot, UsageSnapshotStatus, UsageSource,
    };

    let mut view = bound_usage_view_for_test(BoundUsageFixture {
        capability,
        agent: "claude-alt",
        provider: "Anthropic",
        account_label: "shared@example.test",
        plan_label: Some("B Max 20x"),
        fetched_at: 2_000,
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::Cli,
        status_bar_label: "B-only summary",
        last_error: Some("B own provider warning"),
    });
    view.account.username = Some("b-user".to_owned());
    view.account.credential_origin = Some("B synthetic source".to_owned());
    view.buckets = vec![
        QuotaBucketView {
            label: "B requests".to_owned(),
            used_label: Some("9007199254740993 used".to_owned()),
            limit_label: Some("9007199254741000 total".to_owned()),
            used_percent: Some(CanonicalUsedPercent::new(123.456_789).unwrap()),
            used_percent_origin: Some(PercentObservationOrigin::Derived),
            remaining_percent: Some(CanonicalRemainingPercent::new(0.125_000_1).unwrap()),
            remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            count_quota: Some(CountQuota::new(
                Some(9_007_199_254_740_993),
                Some(9_007_199_254_741_000),
                Some(7),
                CountQuotaUnit::Requests,
                CountQuotaPeriod::UtcDaily,
                CountQuotaProvenance::ProviderReported,
            )),
            provider_quota_state: Some(ProviderQuotaState::Unlimited),
            reset_label: Some("B reset label".to_owned()),
            resets_at: Some(20_000),
            status_slot: Some(StatusSlot::Daily),
            pace_label: Some("B pace".to_owned()),
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::Warn,
        },
        QuotaBucketView {
            label: "B spend".to_owned(),
            used_label: Some("$123.45 used".to_owned()),
            limit_label: Some("$200.00 total".to_owned()),
            used_percent: None,
            used_percent_origin: None,
            remaining_percent: None,
            remaining_percent_origin: None,
            count_quota: None,
            provider_quota_state: None,
            reset_label: Some("B spend reset".to_owned()),
            resets_at: Some(21_000),
            status_slot: Some(StatusSlot::Spend),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: Some(Money::new(12_345, "USD", 2)),
            limit_money: Some(Money::new(20_000, "USD", 2)),
            severity: jackin_protocol::control::UsageSeverity::Normal,
        },
    ];
    view
}

fn account_c_view(
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> jackin_protocol::control::FocusedUsageView {
    use jackin_protocol::control::{UsageSnapshotStatus, UsageSource};

    bound_usage_view_for_test(BoundUsageFixture {
        capability,
        agent: "codex",
        provider: "OpenAI",
        account_label: "codex@example.test",
        plan_label: Some("C plan"),
        fetched_at: 1_500,
        status: UsageSnapshotStatus::Error,
        source: UsageSource::Cache,
        status_bar_label: "C status owner",
        last_error: Some("C only error"),
    })
}

fn renamed_c_view(
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> jackin_protocol::control::FocusedUsageView {
    use jackin_protocol::control::{UsageSnapshotStatus, UsageSource};

    bound_usage_view_for_test(BoundUsageFixture {
        capability,
        agent: "codex",
        provider: "OpenAI",
        account_label: "codex-renamed@example.test",
        plan_label: Some("C updated plan"),
        fetched_at: 2_500,
        status: UsageSnapshotStatus::NeedsLogin,
        source: UsageSource::Cli,
        status_bar_label: "C updated status",
        last_error: Some("C updated error"),
    })
}

fn account_d_view(
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> jackin_protocol::control::FocusedUsageView {
    use jackin_protocol::control::{UsageSnapshotStatus, UsageSource};

    bound_usage_view_for_test(BoundUsageFixture {
        capability,
        agent: "gemini",
        provider: "Google",
        account_label: "google@example.test",
        plan_label: Some("D plan"),
        fetched_at: 2_600,
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        status_bar_label: "D status",
        last_error: None,
    })
}

fn returned_b_view(
    capability: &jackin_protocol::usage_broker::UsageAccountCapability,
) -> jackin_protocol::control::FocusedUsageView {
    use jackin_protocol::control::{
        CanonicalRemainingPercent, CanonicalUsedPercent, CountQuota, CountQuotaPeriod,
        CountQuotaProvenance, CountQuotaUnit, PercentObservationOrigin, ProviderQuotaState,
        QuotaBucketView, StatusSlot, UsageSnapshotStatus, UsageSource,
    };

    let mut view = bound_usage_view_for_test(BoundUsageFixture {
        capability,
        agent: "claude-alt",
        provider: "Anthropic",
        account_label: "b-renamed@example.test",
        plan_label: Some("B renamed plan"),
        fetched_at: 3_000,
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        status_bar_label: "B returned status",
        last_error: Some("B returned error"),
    });
    view.buckets = vec![QuotaBucketView {
        label: "B returned".to_owned(),
        used_label: None,
        limit_label: None,
        used_percent: Some(CanonicalUsedPercent::new(42.5).unwrap()),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        remaining_percent: Some(CanonicalRemainingPercent::new(57.5).unwrap()),
        remaining_percent_origin: Some(PercentObservationOrigin::Derived),
        count_quota: Some(CountQuota::new(
            Some(12),
            Some(21),
            Some(9),
            CountQuotaUnit::Requests,
            CountQuotaPeriod::UtcDaily,
            CountQuotaProvenance::ProviderReported,
        )),
        provider_quota_state: Some(ProviderQuotaState::Exhausted),
        reset_label: Some("returned B reset".to_owned()),
        resets_at: Some(30_000),
        status_slot: Some(StatusSlot::Daily),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: jackin_protocol::control::UsageSeverity::Normal,
    }];
    view
}

struct SelectedAccountScenario {
    mux: Multiplexer,
    capability_a: jackin_protocol::usage_broker::UsageAccountCapability,
    capability_b: jackin_protocol::usage_broker::UsageAccountCapability,
    capability_c: jackin_protocol::usage_broker::UsageAccountCapability,
    capability_d: jackin_protocol::usage_broker::UsageAccountCapability,
}

struct UnavailableSelectionProof {
    unavailable_at: i64,
    unavailable_error: Option<String>,
    unavailable_view: jackin_protocol::control::FocusedUsageView,
    codex_tab: jackin_protocol::control::UsageProviderTab,
}

fn prepare_selected_account_scenario() -> SelectedAccountScenario {
    use jackin_protocol::usage_broker::UsageAccountCapability;

    let capability_a = UsageAccountCapability {
        account_id: "account-a".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let capability_b = UsageAccountCapability {
        account_id: "account-b".to_owned(),
        surface_id: "claude".to_owned(),
    };
    let capability_c = UsageAccountCapability {
        account_id: "account-c".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let capability_d = UsageAccountCapability {
        account_id: "account-d".to_owned(),
        surface_id: "google".to_owned(),
    };

    let mut mux = single_pane_tab_mux();
    install_usage_session_for_test(&mut mux, 1, "claude", "Anthropic", &capability_a);
    install_usage_session_for_test(&mut mux, 2, "claude-alt", "Anthropic", &capability_b);
    install_usage_session_for_test(&mut mux, 3, "codex", "OpenAI", &capability_c);
    mux.launch_env.launch_config.usage_capabilities = BTreeMap::from([
        ("claude".to_owned(), capability_a.clone()),
        ("claude-alt".to_owned(), capability_b.clone()),
        ("codex".to_owned(), capability_c.clone()),
    ]);
    mux.session_supervisor.tabs[0] = Tab::new_single("Claude", 1, "test");

    let view_a = account_a_view(&capability_a);
    let view_b = account_b_view(&capability_b);
    let view_c = account_c_view(&capability_c);

    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test(
            "claude",
            Some("Anthropic"),
            &capability_a,
            view_a,
        );
    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test(
            "claude-alt",
            Some("Anthropic"),
            &capability_b,
            view_b,
        );
    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test("codex", Some("OpenAI"), &capability_c, view_c);
    mux.dialog_push(Dialog::new_usage(
        jackin_protocol::control::FocusedUsageView::unavailable("seed", 1),
    ));

    SelectedAccountScenario {
        mux,
        capability_a,
        capability_b,
        capability_c,
        capability_d,
    }
}

fn select_and_assert_bound_b(scenario: &mut SelectedAccountScenario) {
    use jackin_protocol::control::{
        CanonicalRemainingPercent, CanonicalUsedPercent, Money, PercentObservationOrigin,
        ProviderQuotaState, UsageSnapshotStatus, UsageSource,
    };

    let capability_b = scenario.capability_b.clone();
    let mux = &mut scenario.mux;
    let selected_b_id = capability_b.opaque_identity_key();
    mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
        provider_label: "Anthropic · shared@example.test".to_owned(),
        account_id: selected_b_id.clone(),
    });
    let _refreshed = mux.refresh_open_usage_dialog_from_cache();
    let Dialog::Usage {
        view,
        selected: selected_tab,
        ..
    } = mux.dialog_top().expect("selected B dialog")
    else {
        panic!("usage dialog must stay open");
    };
    assert_eq!(
        *selected_tab,
        crate::tui::components::dialog::UsageDialogTab::Provider
    );
    assert_eq!(
        view.account.account_capability.as_ref(),
        Some(&capability_b)
    );
    assert_eq!(view.account.account_label, "shared@example.test");
    assert_eq!(view.account.username.as_deref(), Some("b-user"));
    assert_eq!(view.account.plan_label.as_deref(), Some("B Max 20x"));
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("B synthetic source")
    );
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::Cli);
    assert_eq!(view.last_error.as_deref(), Some("B own provider warning"));
    assert_eq!(view.fetched_at_epoch, 2_000);
    assert_eq!(view.status_bar_label, "B-only summary");
    assert_eq!(view.buckets.len(), 2);
    assert_eq!(
        view.buckets[0].used_percent.map(CanonicalUsedPercent::get),
        Some(123.456_789)
    );
    assert_eq!(
        view.buckets[0].used_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );
    assert_eq!(
        view.buckets[0]
            .remaining_percent
            .map(CanonicalRemainingPercent::get),
        Some(0.125_000_1)
    );
    assert_eq!(
        view.buckets[0].remaining_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        view.buckets[0].count_quota.and_then(|quota| quota.used),
        Some(9_007_199_254_740_993)
    );
    assert_eq!(
        view.buckets[0].provider_quota_state,
        Some(ProviderQuotaState::Unlimited)
    );
    assert_eq!(
        view.buckets[0].reset_label.as_deref(),
        Some("B reset label")
    );
    assert_eq!(view.buckets[0].resets_at, Some(20_000));
    assert_eq!(
        view.buckets[1].used_money,
        Some(Money::new(12_345, "USD", 2))
    );
    assert_eq!(
        view.buckets[1].limit_money,
        Some(Money::new(20_000, "USD", 2))
    );
    let active = view
        .tabs
        .iter()
        .filter(|tab| tab.active)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].account_capability.as_ref(), Some(&capability_b));
}

fn remove_b_and_assert_unavailable(
    scenario: &mut SelectedAccountScenario,
) -> UnavailableSelectionProof {
    use jackin_protocol::control::UsageSnapshotStatus;

    let capability_a = scenario.capability_a.clone();
    let capability_b = scenario.capability_b.clone();
    let capability_c = scenario.capability_c.clone();
    let mux = &mut scenario.mux;
    mux.usage
        .usage_cache
        .snapshots
        .retain(|_, cached| cached.view.account.account_capability.as_ref() != Some(&capability_b));
    assert!(mux.refresh_open_usage_dialog_from_cache());
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("unavailable B dialog") else {
        panic!("usage dialog must stay open");
    };
    assert_eq!(
        view.account.account_capability.as_ref(),
        Some(&capability_b)
    );
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.buckets.len(), 0, "A or C quota must never fill B");
    assert_eq!(
        view.last_error.as_deref(),
        Some("usage unavailable: selected account not cached")
    );
    let unavailable_at = view.fetched_at_epoch;
    let unavailable_error = view.last_error.clone();
    for capability in [&capability_a, &capability_c] {
        assert_eq!(
            view.tabs
                .iter()
                .filter(|tab| tab.account_capability.as_ref() == Some(capability))
                .count(),
            1,
            "every current nonselected account remains navigable once"
        );
    }
    assert_eq!(
        view.tabs
            .iter()
            .filter(|tab| tab.account_capability.as_ref() == Some(&capability_b))
            .count(),
        1,
        "the unresolved B placeholder appears exactly once"
    );
    assert_eq!(
        view.tabs
            .iter()
            .find(|tab| tab.account_capability.as_ref() == Some(&capability_b))
            .map(|tab| (
                tab.label.as_str(),
                tab.plan_label.as_deref(),
                tab.status_label.as_str(),
                tab.source_label.as_deref()
            )),
        Some((
            "Anthropic · shared@example.test",
            Some("B Max 20x"),
            "unavailable",
            Some("unavailable · no source")
        ))
    );
    assert_eq!(
        view.tabs
            .iter()
            .find(|tab| tab.account_capability.as_ref() == Some(&capability_a))
            .map(|tab| (tab.status_label.as_str(), tab.source_label.as_deref())),
        Some(("stale", Some("stale · cache")))
    );
    assert_eq!(
        view.tabs
            .iter()
            .find(|tab| tab.account_capability.as_ref() == Some(&capability_c))
            .map(|tab| (tab.status_label.as_str(), tab.source_label.as_deref())),
        Some(("error", Some("error · cache")))
    );
    let active = view
        .tabs
        .iter()
        .filter(|tab| tab.active)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].account_capability.as_ref(), Some(&capability_b));

    let unavailable_view = view.as_ref().clone();
    let codex_tab = view
        .tabs
        .iter()
        .find(|tab| tab.account_capability.as_ref() == Some(&capability_c))
        .expect("current Codex tab remains available")
        .clone();

    UnavailableSelectionProof {
        unavailable_at,
        unavailable_error,
        unavailable_view,
        codex_tab,
    }
}

fn navigate_from_unavailable_b_to_codex(
    scenario: &mut SelectedAccountScenario,
    proof: &UnavailableSelectionProof,
) {
    use jackin_protocol::control::UsageSnapshotStatus;

    let capability_c = scenario.capability_c.clone();
    let mux = &mut scenario.mux;
    let overview = Dialog::new_usage_with_tab(
        proof.unavailable_view.clone(),
        crate::tui::components::dialog::UsageDialogTab::Overview,
    );
    let overview_state = overview.usage_state().expect("overview state");
    let overview_values = overview_state
        .rows()
        .iter()
        .map(crate::tui::components::container_info_surface::ContainerInfoRow::value)
        .collect::<Vec<_>>();
    assert!(overview_values.iter().any(|value| value.contains("stale")));
    assert!(overview_values.iter().any(|value| value.contains("error")));
    assert!(
        overview_values
            .iter()
            .any(|value| value.contains("unavailable"))
    );

    let unavailable_view = proof.unavailable_view.clone();
    let codex_tab = proof.codex_tab.clone();
    mux.dialog_push(Dialog::new_usage_with_tab(
        unavailable_view.clone(),
        crate::tui::components::dialog::UsageDialogTab::Overview,
    ));
    let _changed = mux.refresh_open_usage_dialog_from_cache();
    let Dialog::Usage {
        view: overview_view,
        selected,
        ..
    } = mux.dialog_top().expect("overview remains selected")
    else {
        panic!("usage dialog must stay open");
    };
    assert_eq!(
        *selected,
        crate::tui::components::dialog::UsageDialogTab::Overview
    );
    assert!(
        overview_view
            .tabs
            .iter()
            .any(|tab| { tab.account_capability.as_ref() == Some(&capability_c) })
    );
    let restored_b_view = overview_view.as_ref().clone();
    mux.dialog_push(Dialog::new_usage(restored_b_view));
    mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
        provider_label: codex_tab.label,
        account_id: codex_tab.id,
    });
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("switched Codex view") else {
        panic!("usage dialog must stay open");
    };
    assert_eq!(
        view.account.account_capability.as_ref(),
        Some(&capability_c)
    );
    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert_eq!(view.last_error.as_deref(), Some("C only error"));
}

fn update_inventory_while_b_is_unavailable(
    scenario: &mut SelectedAccountScenario,
    proof: &UnavailableSelectionProof,
) {
    let capability_b = scenario.capability_b.clone();
    let capability_c = scenario.capability_c.clone();
    let capability_d = scenario.capability_d.clone();
    let unavailable_at = proof.unavailable_at;
    let unavailable_error = proof.unavailable_error.clone();
    let unavailable_view = proof.unavailable_view.clone();
    let mux = &mut scenario.mux;
    mux.dialog_push(Dialog::new_usage(unavailable_view));
    let mut renamed_c = renamed_c_view(&capability_c);
    renamed_c.buckets.clear();
    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test("codex", Some("OpenAI"), &capability_c, renamed_c);
    let mut view_d = account_d_view(&capability_d);
    view_d.buckets.clear();
    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test("gemini", Some("Google"), &capability_d, view_d);
    assert!(mux.refresh_open_usage_dialog_from_cache());
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("refreshed unavailable B") else {
        panic!("usage dialog must stay open");
    };
    assert_eq!(
        view.account.account_capability.as_ref(),
        Some(&capability_b)
    );
    assert_eq!(view.fetched_at_epoch, unavailable_at);
    assert_eq!(view.last_error, unavailable_error);
    assert!(view.tabs.iter().any(|tab| {
        tab.account_capability.as_ref() == Some(&capability_c)
            && tab.label == "OpenAI · codex-renamed@example.test"
            && tab.plan_label.as_deref() == Some("C updated plan")
            && tab.status_label == "needs login"
            && tab.source_label.as_deref() == Some("needs login · managed CLI")
    }));
    assert!(view.tabs.iter().any(|tab| {
        tab.account_capability.as_ref() == Some(&capability_d)
            && tab.label == "Google · google@example.test"
    }));
    let active = view
        .tabs
        .iter()
        .filter(|tab| tab.active)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].account_capability.as_ref(), Some(&capability_b));
    assert_eq!(
        view.tabs
            .iter()
            .filter(|tab| tab.account_capability.as_ref() == Some(&capability_b))
            .count(),
        1
    );
}

fn remove_a_and_d_while_b_is_unavailable(
    scenario: &mut SelectedAccountScenario,
    proof: &UnavailableSelectionProof,
) {
    let capability_a = scenario.capability_a.clone();
    let capability_b = scenario.capability_b.clone();
    let capability_c = scenario.capability_c.clone();
    let capability_d = scenario.capability_d.clone();
    let unavailable_at = proof.unavailable_at;
    let mux = &mut scenario.mux;
    mux.usage.usage_cache.snapshots.retain(|_, cached| {
        !matches!(
            cached.view.account.account_capability.as_ref(),
            Some(capability) if capability == &capability_a || capability == &capability_d
        )
    });
    assert!(mux.refresh_open_usage_dialog_from_cache());
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("B with only another provider") else {
        panic!("usage dialog must stay open");
    };
    assert_eq!(
        view.account.account_capability.as_ref(),
        Some(&capability_b)
    );
    assert_eq!(view.fetched_at_epoch, unavailable_at);
    assert_eq!(view.buckets.len(), 0);
    assert_eq!(
        view.tabs.len(),
        2,
        "removed A and D must not remain as ghosts"
    );
    assert!(view.tabs.iter().any(|tab| {
        tab.account_capability.as_ref() == Some(&capability_c)
            && tab.label == "OpenAI · codex-renamed@example.test"
    }));
    assert!(view.tabs.iter().any(|tab| {
        tab.account_capability.as_ref() == Some(&capability_b) && tab.status_label == "unavailable"
    }));
    assert_eq!(
        view.tabs
            .iter()
            .filter(|tab| tab.account_capability.as_ref() == Some(&capability_b))
            .count(),
        1
    );
    let active = view
        .tabs
        .iter()
        .filter(|tab| tab.active)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].account_capability.as_ref(), Some(&capability_b));
}

fn restore_b_and_assert_replaced_placeholder(scenario: &mut SelectedAccountScenario) {
    use jackin_protocol::control::{UsageSnapshotStatus, UsageSource};

    let capability_b = scenario.capability_b.clone();
    let mux = &mut scenario.mux;
    let returned_b = returned_b_view(&capability_b);
    mux.usage
        .usage_cache
        .insert_snapshot_for_capability_for_test(
            "claude-alt",
            Some("Anthropic"),
            &capability_b,
            returned_b,
        );
    assert!(mux.refresh_open_usage_dialog_from_cache());
    let Dialog::Usage { view, .. } = mux.dialog_top().expect("restored B dialog") else {
        panic!("usage dialog must stay open");
    };
    assert_eq!(
        view.account.account_capability.as_ref(),
        Some(&capability_b)
    );
    assert_eq!(view.account.account_label, "b-renamed@example.test");
    assert_eq!(view.account.plan_label.as_deref(), Some("B renamed plan"));
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::ProviderApi);
    assert_eq!(view.last_error.as_deref(), Some("B returned error"));
    assert_eq!(view.buckets.len(), 1);
    assert_eq!(
        view.buckets[0].reset_label.as_deref(),
        Some("returned B reset")
    );
    assert_eq!(
        view.tabs
            .iter()
            .filter(|tab| tab.account_capability.as_ref() == Some(&capability_b))
            .count(),
        1,
        "the real same-key observation replaces rather than duplicates its placeholder"
    );
    let active = view
        .tabs
        .iter()
        .filter(|tab| tab.active)
        .collect::<Vec<_>>();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].account_capability.as_ref(), Some(&capability_b));
}

#[test]
fn selected_bound_account_survives_missing_cache_while_live_tabs_track_inventory() {
    let mut scenario = prepare_selected_account_scenario();
    select_and_assert_bound_b(&mut scenario);
    let proof = remove_b_and_assert_unavailable(&mut scenario);
    navigate_from_unavailable_b_to_codex(&mut scenario, &proof);
    update_inventory_while_b_is_unavailable(&mut scenario, &proof);
    remove_a_and_d_while_b_is_unavailable(&mut scenario, &proof);
    restore_b_and_assert_replaced_placeholder(&mut scenario);
}

#[test]
fn account_refresh_search_continues_after_invalid_matching_session() {
    use jackin_protocol::control::FocusedUsageView;
    use jackin_protocol::usage_broker::UsageAccountCapability;

    let capability = UsageAccountCapability {
        account_id: "shared-account".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let same_account_other_surface = UsageAccountCapability {
        account_id: "shared-account".to_owned(),
        surface_id: "claude".to_owned(),
    };

    for invalid_inserted_first in [true, false] {
        let mut mux = single_pane_tab_mux();
        for (agent, provider, selected_capability, label) in [
            ("codex", "OpenAI", &capability, "same@example.test"),
            (
                "claude",
                "Anthropic",
                &same_account_other_surface,
                "same@example.test",
            ),
        ] {
            let mut view = FocusedUsageView::unavailable("fixture", 1_000);
            view.focused_agent = Some(agent.to_owned());
            view.focused_provider = Some(provider.to_owned());
            view.account = jackin_protocol::control::FocusedAccountHeader {
                provider_label: provider.to_owned(),
                account_label: label.to_owned(),
                username: None,
                plan_label: None,
                credential_origin: None,
                account_capability: Some(selected_capability.clone()),
            };
            mux.usage
                .usage_cache
                .insert_snapshot_for_capability_for_test(
                    agent,
                    Some(provider),
                    selected_capability,
                    view,
                );
        }
        mux.launch_env.launch_config.usage_capabilities = BTreeMap::from([
            ("codex".to_owned(), capability.clone()),
            ("claude".to_owned(), same_account_other_surface.clone()),
        ]);

        let (mut first, _first_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
        first.provider = Some(crate::session::SessionProvider {
            label: "OpenAI".to_owned(),
            env_overrides: Vec::new(),
        });
        first.usage_capability = Some(capability.clone());
        let (mut second, _second_rx) = test_session_with_agent(24, 80, Some("codex".to_owned()));
        second.provider = Some(crate::session::SessionProvider {
            label: "OpenAI".to_owned(),
            env_overrides: Vec::new(),
        });
        second.usage_capability = Some(capability.clone());
        if invalid_inserted_first {
            mux.session_supervisor.sessions.insert(10, first);
            mux.session_supervisor.sessions.insert(11, second);
        } else {
            mux.session_supervisor.sessions.insert(11, second);
            mux.session_supervisor.sessions.insert(10, first);
        }

        let ordered: Vec<_> = mux
            .session_supervisor
            .sessions
            .iter()
            .filter(|(_, session)| session.usage_capability.as_ref() == Some(&capability))
            .map(|(id, _)| id)
            .collect();
        assert_eq!(ordered.len(), 2);
        let invalid_id = ordered[0];
        let valid_id = ordered[1];
        mux.session_supervisor
            .sessions
            .get_mut(invalid_id)
            .expect("first exact-capability session")
            .agent = None;
        let valid = mux
            .session_supervisor
            .sessions
            .get_mut(valid_id)
            .expect("later exact-capability session");
        valid.agent = Some("codex".to_owned());
        valid.provider = Some(crate::session::SessionProvider {
            label: "OpenAI".to_owned(),
            env_overrides: Vec::new(),
        });
        valid.usage_capability = Some(capability.clone());

        mux.dialog_push(Dialog::new_usage(FocusedUsageView::unavailable("seed", 1)));
        mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
            provider_label: "OpenAI · same@example.test".to_owned(),
            account_id: capability.opaque_identity_key(),
        });
        assert_eq!(
            mux.usage
                .pending_usage_refresh
                .as_ref()
                .map(|target| (target.agent.as_str(), target.capability.clone())),
            Some(("codex", capability.clone())),
            "a first invalid match cannot mask the later valid exact-tuple session"
        );

        mux.usage.pending_usage_refresh = None;
        mux.launch_env
            .launch_config
            .usage_capabilities
            .remove("codex");
        mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
            provider_label: "OpenAI · same@example.test".to_owned(),
            account_id: capability.opaque_identity_key(),
        });
        assert_eq!(mux.usage.pending_usage_refresh, None);
    }

    let mut wrong_surface_mux = single_pane_tab_mux();
    let (mut wrong_surface, _rx) = test_session_with_agent(24, 80, Some("claude".to_owned()));
    wrong_surface.provider = Some(crate::session::SessionProvider {
        label: "Anthropic".to_owned(),
        env_overrides: Vec::new(),
    });
    wrong_surface.usage_capability = Some(same_account_other_surface.clone());
    wrong_surface_mux
        .session_supervisor
        .sessions
        .insert(10, wrong_surface);
    wrong_surface_mux
        .launch_env
        .launch_config
        .usage_capabilities = BTreeMap::from([("claude".to_owned(), same_account_other_surface)]);
    let mut target_view = FocusedUsageView::unavailable("fixture", 1_000);
    target_view.focused_agent = Some("codex".to_owned());
    target_view.focused_provider = Some("OpenAI".to_owned());
    target_view.account.account_label = "same@example.test".to_owned();
    wrong_surface_mux
        .usage
        .usage_cache
        .insert_snapshot_for_capability_for_test("codex", Some("OpenAI"), &capability, target_view);
    wrong_surface_mux.dialog_push(Dialog::new_usage(FocusedUsageView::unavailable("seed", 1)));
    wrong_surface_mux.apply_dialog_action(DialogAction::SwitchUsageProvider {
        provider_label: "OpenAI · same@example.test".to_owned(),
        account_id: capability.opaque_identity_key(),
    });
    assert_eq!(wrong_surface_mux.usage.pending_usage_refresh, None);
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
            account_capability: None,
        },
        buckets: vec![jackin_protocol::control::QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: jackin_protocol::control::UsageSeverity::default(),
            label: "Session".to_owned(),
            used_label: Some("63% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            used_percent: None,
            used_percent_origin: None,
            remaining_percent: Some(
                jackin_protocol::control::CanonicalRemainingPercent::new(37.0).unwrap(),
            ),
            remaining_percent_origin: Some(
                jackin_protocol::control::PercentObservationOrigin::ProviderReported,
            ),
            count_quota: None,
            provider_quota_state: None,
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
