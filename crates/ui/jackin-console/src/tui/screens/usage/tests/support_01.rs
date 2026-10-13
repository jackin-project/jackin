// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const TEST_NOW_EPOCH: i64 = 1_800_000_000;

pub(super) fn test_window(label: &str, remaining: Option<u8>) -> UsageWindow {
    UsageWindow {
        window_id: format!("{label}-id"),
        rank: 0,
        category: UsageWindowCategoryV1::LongRange,
        label: label.to_owned(),
        value: format!("{label} value"),
        reset: "resets soon".to_owned(),
        remaining_percent: remaining,
        remaining_raw_percent: remaining.map(i32::from),
        used_percent: None,
        used_raw_percent: None,
        reset_at_epoch: Some(1_800_000_000),
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
    }
}

pub(super) fn test_account(provider_id: &str, account_id: &str, label: &str) -> UsageAccount {
    UsageAccount {
        provider_id: provider_id.to_owned(),
        canonical_account_id: account_id.to_owned(),
        unresolved: false,
        provider: provider_id.to_owned(),
        account: label.to_owned(),
        status: "available".to_owned(),
        lifecycle: UsageLifecycleV1::Available,
        freshness_phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(1_799_999_000),
        retry_at_epoch: None,
        is_stale: false,
        identity_kind: Some(UsageIdentityKindV1::ProviderStableHandle),
        plan_label: None,
        credential_expires_at_epoch: None,
        issues: Vec::new(),
        provider_issues: Vec::new(),
        windows: vec![test_window("weekly", Some(73))],
        metric_groups: Vec::new(),
    }
}

pub(super) fn metric_group_fixture(
    group_id: &str,
    rank: u32,
    kind: jackin_protocol::usage_broker::UsageMetricGroupKindV1,
    label: &str,
    value: jackin_protocol::usage_broker::UsageMetricValueV1,
    now: i64,
) -> jackin_protocol::usage_broker::UsageMetricGroupV1 {
    use jackin_protocol::usage_broker::{
        UsageFreshnessPhaseV1, UsageMetricScopeV1, UsageQuotaStateV1,
    };
    jackin_protocol::usage_broker::UsageMetricGroupV1 {
        group_id: group_id.to_owned(),
        rank,
        kind,
        label: label.to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: now - 10,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value,
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    }
}

pub(super) struct MetricGroupEpochs {
    pub(super) retry_at: i64,
    pub(super) credential_expires_at: i64,
    pub(super) reset_at: i64,
    pub(super) renews_at: i64,
}

pub(super) fn metric_group_projection_fixture() -> (
    jackin_protocol::usage_broker::UsageProjectionV1,
    MetricGroupEpochs,
) {
    use jackin_protocol::control::Money;
    use jackin_protocol::usage_broker::{
        UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
        UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
        UsageLimitWindowV1, UsageMembershipStateV1, UsageMetricGroupKindV1, UsageMetricValueV1,
        UsagePercent, UsageProjectionRefreshStateV1, UsageProjectionSchemaV1, UsageProjectionV1,
        UsageProviderV1, UsageQuotaStateV1, UsageWindowCategoryV1,
    };

    let now = TEST_NOW_EPOCH;
    let epochs = MetricGroupEpochs {
        retry_at: now + 120,
        credential_expires_at: now + 30 * 86_400,
        reset_at: now + 86_400,
        renews_at: now + 5 * 86_400,
    };

    let issue = |code: &str, scope, message: &str| UsageIssueV1 {
        code: code.to_owned(),
        scope,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: message.to_owned(),
        retry_at_epoch: None,
    };
    let freshness = || UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(now - 300),
        retry_at_epoch: None,
        is_stale: false,
    };

    let mut balance = metric_group_fixture(
        "balance",
        0,
        UsageMetricGroupKindV1::Balance,
        "Balance",
        UsageMetricValueV1::Balance {
            amount: Money::new(4_250, "USD", 2),
            expires_at_epoch: None,
        },
        now,
    );
    balance.scope.model = Some("gpt-5".to_owned());
    balance.last_success_at_epoch = Some(now - 300);
    balance.issues = vec![issue(
        "bal_delay",
        UsageIssueScopeV1::Group,
        "balance delayed",
    )];
    let mut spend = metric_group_fixture(
        "spend",
        1,
        UsageMetricGroupKindV1::SpendCap,
        "Spend cap",
        UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(30_000, "USD", 2)),
            spent: Some(Money::new(5_331, "USD", 2)),
            remaining: None,
        },
        now,
    );
    spend.reset_at_epoch = Some(epochs.reset_at);
    let mut plan = metric_group_fixture(
        "plan",
        2,
        UsageMetricGroupKindV1::Plan,
        "Plan",
        UsageMetricValueV1::Plan {
            plan_label: Some("Pro".to_owned()),
            tier: None,
        },
        now,
    );
    plan.quota_state = UsageQuotaStateV1::NotApplicable;
    plan.renews_at_epoch = Some(epochs.renews_at);

    let projection = UsageProjectionV1 {
        schema_version: UsageProjectionSchemaV1,
        projection_id: "p".to_owned(),
        generated_at_epoch: now,
        discovery_revision: "d".to_owned(),
        broker_instance_id: "b".to_owned(),
        broker_generation: 1,
        refresh_state: UsageProjectionRefreshStateV1::Idle,
        providers: vec![UsageProviderV1 {
            provider_id: "openai".to_owned(),
            display_name: "OpenAI".to_owned(),
            rank: 0,
            membership_state: UsageMembershipStateV1::Current,
            freshness: freshness(),
            accounts: vec![UsageAccountV1 {
                canonical_account_id: "canon-1".to_owned(),
                identity_kind: UsageIdentityKindV1::ProviderAccountId,
                rank: 0,
                display_label: "work".to_owned(),
                plan_label: Some("Scale".to_owned()),
                status_label: None,
                lifecycle: UsageLifecycleV1::Available,
                freshness: UsageFreshnessV1 {
                    retry_at_epoch: Some(epochs.retry_at),
                    ..freshness()
                },
                provenance_count: 1,
                windows: vec![UsageLimitWindowV1 {
                    window_id: "weekly".to_owned(),
                    rank: 0,
                    category: UsageWindowCategoryV1::LongRange,
                    label: "weekly".to_owned(),
                    value_label: "120% used".to_owned(),
                    reset_label: "resets at midnight".to_owned(),
                    remaining_percent: None,
                    remaining_raw_percent: None,
                    used_percent: Some(UsagePercent::new(100).expect("valid percent")),
                    used_raw_percent: Some(120),
                    reset_at_epoch: None,
                    quota_state: UsageQuotaStateV1::Warning,
                    pace_label: Some("on pace".to_owned()),
                    runs_out_label: None,
                }],
                metric_groups: vec![balance, spend, plan],
                credential_expires_at_epoch: Some(epochs.credential_expires_at),
                issues: vec![issue(
                    "quota_degraded",
                    UsageIssueScopeV1::Account,
                    "quota degraded",
                )],
            }],
            issues: vec![issue(
                "prov_maint",
                UsageIssueScopeV1::Provider,
                "provider maintenance",
            )],
        }],
        unresolved: Vec::new(),
        issues: vec![issue(
            "proj_wide",
            UsageIssueScopeV1::Projection,
            "projection wide fault",
        )],
    };

    (projection, epochs)
}

pub(super) fn expect_rows(haystack: &str, context: &str, expected: &[&str]) {
    for row in expected {
        assert!(
            haystack.contains(row),
            "{context} missing {row:?}:\n{haystack}"
        );
    }
}

pub(super) fn assert_detail_group_rows(detail: &str) {
    expect_rows(
        detail,
        "detail",
        &[
            "Plan      Scale",
            "Identity  provider account id",
            "Credential expires in 30d",
            "Freshness updated 5m ago · retry in 2m",
            "Metric groups",
            "Balance (balance · available · updated 5m ago)",
            "scope: model gpt-5",
            "$42.50",
            "balance delayed (bal_delay)",
            "Spend cap (spend cap · available · never updated)",
            "cap $300.00 · spent $53.31",
            "resets in 1d",
            "Plan (plan · n/a · never updated)",
            "Pro",
            "renews in 5d",
            "quota: warning",
            "raw used 120%",
            "pace: on pace",
            "Issues",
            "quota degraded (quota_degraded)",
            "provider: provider maintenance (prov_maint)",
        ],
    );
    // No invented values: absent money/tiers/expiry/renewal/reset render nothing.
    assert!(
        !detail.contains("remaining"),
        "unreported remaining must not render:\n{detail}"
    );
    assert!(
        !detail.contains("tier"),
        "unreported tier must not render:\n{detail}"
    );
    assert!(
        !detail.contains("uncapped"),
        "reported cap must not read uncapped:\n{detail}"
    );
    assert_eq!(
        detail.matches("expires").count(),
        1,
        "only the credential expiry may use `expires`:\n{detail}"
    );
    assert_eq!(
        detail.matches("resets in").count(),
        1,
        "reset must render exactly once, on the spend-cap group:\n{detail}"
    );
    assert_eq!(
        detail.matches("renews in").count(),
        1,
        "renewal must render exactly once, on the plan group:\n{detail}"
    );
}

pub(super) fn assert_overview_group_rows(overview: &str) {
    expect_rows(
        overview,
        "overview",
        &[
            "Plan Scale",
            "Credential expires in 30d",
            "Balance: $42.50",
            "Spend cap: cap $300.00 · spent $53.31",
            "Plan: Pro",
            "resets in 1d",
            "renews in 5d",
            "[Balance] balance delayed (bal_delay)",
            "quota degraded (quota_degraded)",
            "projection wide fault (proj_wide)",
        ],
    );
}

pub(super) fn manager_with_usage(
    state: UsageScreenState,
) -> crate::tui::state::ManagerState<'static> {
    let config = jackin_config::AppConfig::default();
    let mut manager =
        crate::tui::state::ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(state);
    manager
}

pub(super) fn render_detail_text(state: UsageScreenState) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let manager = manager_with_usage(state);
    let backend = TestBackend::new(120, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_detail(f, f.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();
    backend_text(&terminal)
}

pub(super) fn render_list_text(state: UsageScreenState) -> String {
    use ratatui::{Terminal, backend::TestBackend};
    let manager = manager_with_usage(state);
    let backend = TestBackend::new(120, 60);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render_account_list(f, f.area(), &manager, TEST_NOW_EPOCH))
        .unwrap();
    backend_text(&terminal)
}

pub(super) fn press_key(
    manager: &mut crate::tui::state::ManagerState<'_>,
    code: crossterm::event::KeyCode,
) {
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    handle_key(
        manager,
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        },
    );
}

pub(super) fn usage_issue(
    code: &str,
    message: &str,
) -> jackin_protocol::usage_broker::UsageIssueV1 {
    use jackin_protocol::usage_broker::{
        UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1,
    };
    UsageIssueV1 {
        code: code.to_owned(),
        scope: UsageIssueScopeV1::Account,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: message.to_owned(),
        retry_at_epoch: None,
    }
}
