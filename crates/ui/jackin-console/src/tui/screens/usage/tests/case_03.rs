// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    MeterPercentPoints, Money, PercentObservationOrigin, RawPercentPoints, UsageMeterSide,
};
use jackin_protocol::usage_broker::{
    UsageAccountV1, UsageFreshnessPhaseV1, UsageFreshnessV1, UsageIdentityKindV1,
    UsageIssueRecoverabilityV1, UsageIssueScopeV1, UsageIssueV1, UsageLifecycleV1,
    UsageLimitWindowV1, UsageMembershipStateV1, UsageMetricGroupKindV1, UsageMetricGroupV1,
    UsageMetricPeriodV1, UsageMetricScopeV1, UsageMetricValueV1, UsageProjectionRefreshStateV1,
    UsageProjectionSchemaV2, UsageProjectionV1, UsageProviderV1, UsageQuotaStateV1,
    UsageWindowCategoryV1,
};
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn spend_cap_summary_preserves_exact_money_digits() {
    use jackin_protocol::control::Money;
    use jackin_protocol::usage_broker::{
        UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricScopeV1, UsageMetricValueV1,
        UsageQuotaStateV1,
    };

    let group = UsageMetricGroup {
        group_id: "spend".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: "Spend cap".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: TEST_NOW_EPOCH,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100_000_000_000_000_001, "USD", 18)),
            spent: Some(Money::new(100_000_000_000_000_000, "USD", 18)),
            remaining: Some(Money::new(1, "USD", 18)),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };

    assert_eq!(
        metric_group_value_summary(&group).as_deref(),
        Some(
            "cap $0.100000000000000001 · spent $0.100000000000000000 · remaining $0.000000000000000001"
        )
    );
}
#[test]
fn spend_cap_summary_preserves_mixed_money_scales() {
    use jackin_protocol::control::Money;
    use jackin_protocol::usage_broker::{
        UsageFreshnessPhaseV1, UsageMetricGroupKindV1, UsageMetricScopeV1, UsageMetricValueV1,
        UsageQuotaStateV1,
    };

    let group = UsageMetricGroup {
        group_id: "spend-mixed-scale".to_owned(),
        rank: 0,
        kind: UsageMetricGroupKindV1::SpendCap,
        label: "Spend cap".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: TEST_NOW_EPOCH,
        last_success_at_epoch: None,
        phase: UsageFreshnessPhaseV1::Current,
        is_stale: false,
        quota_state: UsageQuotaStateV1::Available,
        value: UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(100_000_000_000_000_000, "USD", 0)),
            spent: Some(Money::new(999_999_999_999_999_999, "USD", 2)),
            remaining: Some(Money::new(9_000_000_000_000_000_001, "USD", 2)),
        },
        reset_at_epoch: None,
        renews_at_epoch: None,
        issues: Vec::new(),
    };

    assert_eq!(
        metric_group_value_summary(&group).as_deref(),
        Some(
            "cap $100000000000000000 · spent $9999999999999999.99 · remaining $90000000000000000.01"
        )
    );
}

#[test]
fn public_usage_render_preserves_overview_refresh_and_unknown_window_values() {
    let mut openai = test_usage_account();
    openai.windows = vec![
        remaining_window(
            "session",
            UsageWindowCategoryV1::Session,
            "5h session",
            "10% left",
            "resets in 2h",
            10.0,
        ),
        remaining_window(
            "weekly",
            UsageWindowCategoryV1::LongRange,
            "weekly",
            "80% left",
            "resets in 5d",
            80.0,
        ),
    ];
    let mut unresolved = test_usage_account();
    unresolved.provider_id = "anthropic".to_owned();
    unresolved.canonical_account_id = "claude:key".to_owned();
    unresolved.provider = "Anthropic / Claude".to_owned();
    unresolved.account = "Unresolved (claude:key)".to_owned();
    unresolved.status = "needs login · authentication required".to_owned();
    unresolved.lifecycle = UsageLifecycleV1::NeedsLogin;
    unresolved.last_good_at_epoch = None;
    unresolved.unresolved = true;
    unresolved.identity_kind = None;
    let overview = render_usage_text(
        UsageScreenState {
            accounts: vec![openai, unresolved],
            selected: 0,
            notice: Some("1 configured capability(s) unresolved".to_owned()),
            ..UsageScreenState::default()
        },
        120,
        60,
    );
    assert_rows(
        &overview,
        &[
            "OpenAI · work",
            "5h session",
            "10% left · resets in 2h",
            "weekly",
            "80% left · resets in 5d",
            "Anthropic / Claude · Unresolved (claude:key)",
            "needs login · authentication required",
            "1 configured capability(s) unresolved",
        ],
    );

    let minimal = UsageScreenState {
        accounts: vec![test_usage_account()],
        selected: 0,
        notice: Some("1 configured capability(s) unresolved".to_owned()),
        ..UsageScreenState::default()
    };
    for (width, height, notice) in [
        (40, 20, "unresolved"),
        (120, 30, "1 configured capability(s) unresolved"),
    ] {
        let text = render_usage_text(minimal.clone(), width, height);
        assert!(
            text.contains("OpenAI · work"),
            "missing header at {width}x{height}"
        );
        assert!(
            text.contains(notice),
            "missing notice at {width}x{height}:\n{text}"
        );
    }

    let mut stale = test_usage_account();
    stale.last_good_at_epoch = Some(TEST_NOW_EPOCH - 60);
    stale.freshness_phase = UsageFreshnessPhaseV1::Stale;
    stale.is_stale = true;
    let mut refreshing = UsageScreenState {
        accounts: vec![stale],
        selected: 1,
        selected_id: Some("openai:work-id".to_owned()),
        detail: true,
        ..UsageScreenState::default()
    };
    refreshing.begin_refresh(crate::tui::runtime::ready_blocking_subscription((
        refreshing.refresh_generation,
        Ok(UsageScreenState::default()),
    )));
    let refreshing_text = render_usage_text(refreshing, 120, 30);
    assert!(refreshing_text.contains("Freshness stale · updated"));
    assert!(refreshing_text.contains("Refreshing usage…"));

    let mut unknown = test_usage_account();
    unknown.windows = vec![unknown_window()];
    let unknown_text = render_usage_text(
        UsageScreenState {
            accounts: vec![unknown],
            selected: 1,
            selected_id: Some("openai:work-id".to_owned()),
            ..UsageScreenState::default()
        },
        120,
        30,
    );
    assert!(unknown_text.contains("mystery window"));
    assert!(unknown_text.contains("provider did not report"));
    assert!(
        !unknown_text.contains('█') && !unknown_text.contains('░'),
        "unknown quota must not render a bar:\n{unknown_text}"
    );
}

#[test]
fn public_projection_preserves_metric_values_scopes_and_rendered_labels() {
    let (projection, epochs) = metric_group_projection_fixture();
    let state = UsageScreenState::from_projection(&projection);
    assert_projection_values(&state, epochs);
    assert_metric_group_meters(&state.accounts[0]);

    let detail = render_usage_text(
        UsageScreenState {
            accounts: state.accounts.clone(),
            selected: 1,
            selected_id: Some("openai:canon-1".to_owned()),
            detail: true,
            ..UsageScreenState::default()
        },
        120,
        60,
    );
    assert_detail_projection_rows(&detail);

    let overview = render_usage_text(
        UsageScreenState {
            accounts: state.accounts,
            projection_issues: state.projection_issues,
            selected: 0,
            ..UsageScreenState::default()
        },
        120,
        60,
    );
    assert_overview_projection_rows(&overview);
}

#[derive(Clone, Copy)]
struct ProjectionEpochs {
    retry_at: i64,
    credential_expires_at: i64,
    reset_at: i64,
    renews_at: i64,
}

fn projection_epochs() -> ProjectionEpochs {
    ProjectionEpochs {
        retry_at: TEST_NOW_EPOCH + 120,
        credential_expires_at: TEST_NOW_EPOCH + 30 * 86_400,
        reset_at: TEST_NOW_EPOCH + 86_400,
        renews_at: TEST_NOW_EPOCH + 5 * 86_400,
    }
}

fn projection_issue(code: &str, scope: UsageIssueScopeV1, message: &str) -> UsageIssueV1 {
    UsageIssueV1 {
        code: code.to_owned(),
        scope,
        recoverability: UsageIssueRecoverabilityV1::Retryable,
        message: message.to_owned(),
        retry_at_epoch: None,
    }
}

fn projection_freshness(retry_at_epoch: Option<i64>) -> UsageFreshnessV1 {
    UsageFreshnessV1 {
        generation: 1,
        phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(TEST_NOW_EPOCH - 300),
        retry_at_epoch,
        is_stale: false,
    }
}

fn projection_metric_group(
    group_id: &str,
    rank: u32,
    kind: UsageMetricGroupKindV1,
    label: &str,
    value: UsageMetricValueV1,
) -> UsageMetricGroupV1 {
    UsageMetricGroupV1 {
        group_id: group_id.to_owned(),
        rank,
        kind,
        label: label.to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: TEST_NOW_EPOCH - 10,
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

fn projection_metric_groups(epochs: ProjectionEpochs) -> Vec<UsageMetricGroupV1> {
    let mut balance = projection_metric_group(
        "balance",
        0,
        UsageMetricGroupKindV1::Balance,
        "Balance",
        UsageMetricValueV1::Balance {
            amount: Money::new(4_250, "USD", 2),
            expires_at_epoch: None,
        },
    );
    balance.scope.model = Some("gpt-5".to_owned());
    balance.last_success_at_epoch = Some(TEST_NOW_EPOCH - 300);
    balance.issues = vec![projection_issue(
        "bal_delay",
        UsageIssueScopeV1::Group,
        "balance delayed",
    )];
    let mut spend = projection_metric_group(
        "spend",
        1,
        UsageMetricGroupKindV1::SpendCap,
        "Spend cap",
        UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(30_000, "USD", 2)),
            spent: Some(Money::new(5_331, "USD", 2)),
            remaining: None,
        },
    );
    spend.reset_at_epoch = Some(epochs.reset_at);
    let mut plan = projection_metric_group(
        "plan",
        2,
        UsageMetricGroupKindV1::Plan,
        "Plan",
        UsageMetricValueV1::Plan {
            plan_label: Some("Pro".to_owned()),
            tier: None,
        },
    );
    plan.quota_state = UsageQuotaStateV1::NotApplicable;
    plan.renews_at_epoch = Some(epochs.renews_at);
    vec![balance, spend, plan]
}

fn projection_window() -> UsageLimitWindowV1 {
    UsageLimitWindowV1 {
        window_id: "weekly".to_owned(),
        rank: 0,
        category: UsageWindowCategoryV1::LongRange,
        label: "weekly".to_owned(),
        value_label: "120% used".to_owned(),
        reset_label: "resets at midnight".to_owned(),
        remaining_percent: None,
        remaining_percent_origin: None,
        used_percent: Some(RawPercentPoints::new(120.0).expect("valid raw percent")),
        used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        meter_percent: Some(MeterPercentPoints::new(100.0).expect("valid meter")),
        meter_side: Some(UsageMeterSide::Used),
        count_quota: None,
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Warning,
        pace_label: Some("on pace".to_owned()),
        runs_out_label: None,
    }
}

fn projection_account(epochs: ProjectionEpochs) -> UsageAccountV1 {
    UsageAccountV1 {
        canonical_account_id: "canon-1".to_owned(),
        identity_kind: UsageIdentityKindV1::ProviderAccountId,
        rank: 0,
        display_label: "work".to_owned(),
        plan_label: Some("Scale".to_owned()),
        status_label: None,
        lifecycle: UsageLifecycleV1::Available,
        freshness: projection_freshness(Some(epochs.retry_at)),
        provenance_count: 1,
        windows: vec![projection_window()],
        metric_groups: projection_metric_groups(epochs),
        credential_expires_at_epoch: Some(epochs.credential_expires_at),
        issues: vec![projection_issue(
            "quota_degraded",
            UsageIssueScopeV1::Account,
            "quota degraded",
        )],
    }
}

fn metric_group_projection_fixture() -> (UsageProjectionV1, ProjectionEpochs) {
    let epochs = projection_epochs();
    (
        UsageProjectionV1 {
            schema_version: UsageProjectionSchemaV2,
            projection_id: "p".to_owned(),
            generated_at_epoch: TEST_NOW_EPOCH,
            discovery_revision: "d".to_owned(),
            broker_instance_id: "b".to_owned(),
            broker_generation: 1,
            refresh_state: UsageProjectionRefreshStateV1::Idle,
            providers: vec![UsageProviderV1 {
                provider_id: "openai".to_owned(),
                display_name: "OpenAI".to_owned(),
                rank: 0,
                membership_state: UsageMembershipStateV1::Current,
                freshness: projection_freshness(None),
                accounts: vec![projection_account(epochs)],
                issues: vec![projection_issue(
                    "prov_maint",
                    UsageIssueScopeV1::Provider,
                    "provider maintenance",
                )],
            }],
            unresolved: Vec::new(),
            issues: vec![projection_issue(
                "proj_wide",
                UsageIssueScopeV1::Projection,
                "projection wide fault",
            )],
        },
        epochs,
    )
}

fn test_usage_account() -> UsageAccount {
    UsageAccount {
        provider_id: "openai".to_owned(),
        canonical_account_id: "work-id".to_owned(),
        unresolved: false,
        provider: "OpenAI".to_owned(),
        account: "work".to_owned(),
        status: "available".to_owned(),
        lifecycle: UsageLifecycleV1::Available,
        freshness_phase: UsageFreshnessPhaseV1::Current,
        last_good_at_epoch: Some(TEST_NOW_EPOCH - 300),
        retry_at_epoch: None,
        is_stale: false,
        identity_kind: Some(UsageIdentityKindV1::ProviderStableHandle),
        plan_label: None,
        credential_expires_at_epoch: None,
        issues: Vec::new(),
        provider_issues: Vec::new(),
        windows: Vec::new(),
        metric_groups: Vec::new(),
    }
}

fn remaining_window(
    window_id: &str,
    category: UsageWindowCategoryV1,
    label: &str,
    value: &str,
    reset: &str,
    remaining: f64,
) -> UsageWindow {
    let percent = RawPercentPoints::new(remaining).expect("valid raw percent");
    let meter = MeterPercentPoints::new(remaining).expect("valid meter");
    UsageWindow {
        window_id: window_id.to_owned(),
        rank: 0,
        category,
        label: label.to_owned(),
        value: value.to_owned(),
        reset: reset.to_owned(),
        remaining_percent: Some(percent),
        remaining_percent_origin: Some(PercentObservationOrigin::ProviderReported),
        used_percent: None,
        used_percent_origin: None,
        meter_percent: Some(meter),
        meter_side: Some(UsageMeterSide::Remaining),
        count_quota: None,
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Available,
        pace_label: None,
    }
}

fn unknown_window() -> UsageWindow {
    UsageWindow {
        window_id: "mystery".to_owned(),
        rank: 0,
        category: UsageWindowCategoryV1::Other,
        label: "mystery window".to_owned(),
        value: "provider did not report".to_owned(),
        reset: String::new(),
        remaining_percent: None,
        remaining_percent_origin: None,
        used_percent: None,
        used_percent_origin: None,
        meter_percent: None,
        meter_side: None,
        count_quota: None,
        provider_quota_state: None,
        money_used_percent: None,
        reset_at_epoch: None,
        quota_state: UsageQuotaStateV1::Unknown,
        pace_label: None,
    }
}

fn render_usage_text(state: UsageScreenState, width: u16, height: u16) -> String {
    let config = jackin_config::AppConfig::default();
    let mut manager =
        crate::tui::state::ManagerState::from_config(&config, std::path::Path::new("/test"));
    manager.usage.screen = Some(state);
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render_at(frame, frame.area(), &manager, TEST_NOW_EPOCH))
        .expect("render usage screen");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_rows(text: &str, expected: &[&str]) {
    for row in expected {
        assert!(text.contains(row), "missing {row:?}:\n{text}");
    }
}

fn assert_projection_values(state: &UsageScreenState, epochs: ProjectionEpochs) {
    assert_eq!(state.accounts.len(), 1);
    let account = &state.accounts[0];
    assert_eq!(account.plan_label.as_deref(), Some("Scale"));
    assert_eq!(
        account.identity_kind,
        Some(UsageIdentityKindV1::ProviderAccountId)
    );
    assert_eq!(
        account.credential_expires_at_epoch,
        Some(epochs.credential_expires_at)
    );
    assert_eq!(account.retry_at_epoch, Some(epochs.retry_at));
    assert_eq!(account.issues.len(), 1);
    assert_eq!(account.issues[0].code, "quota_degraded");
    assert_eq!(account.provider_issues.len(), 1);
    assert_eq!(account.provider_issues[0].code, "prov_maint");
    assert_eq!(account.issue_count(), 3);
    assert_eq!(state.projection_issues.len(), 1);
    let window = &account.windows[0];
    assert_eq!(window.used_percent.map(RawPercentPoints::get), Some(120.0));
    assert_eq!(
        window.used_percent_origin,
        Some(PercentObservationOrigin::ProviderReported)
    );
    assert_eq!(
        window.meter_percent().map(MeterPercentPoints::get),
        Some(100.0)
    );
    assert_eq!(window.meter_side, Some(UsageMeterSide::Used));
    assert_eq!(window.quota_state, UsageQuotaStateV1::Warning);
    assert_eq!(window.pace_label.as_deref(), Some("on pace"));
    assert_eq!(account.metric_groups.len(), 3);
    let (balance, spend, plan) = (
        &account.metric_groups[0],
        &account.metric_groups[1],
        &account.metric_groups[2],
    );
    assert_eq!(balance.kind, UsageMetricGroupKindV1::Balance);
    assert_eq!(balance.scope.model.as_deref(), Some("gpt-5"));
    assert_eq!(
        balance.value,
        UsageMetricValueV1::Balance {
            amount: Money::new(4_250, "USD", 2),
            expires_at_epoch: None,
        }
    );
    assert_eq!(balance.issues.len(), 1);
    assert_eq!(spend.kind, UsageMetricGroupKindV1::SpendCap);
    assert_eq!(
        spend.value,
        UsageMetricValueV1::SpendCap {
            cap: Some(Money::new(30_000, "USD", 2)),
            spent: Some(Money::new(5_331, "USD", 2)),
            remaining: None,
        }
    );
    assert_eq!(spend.reset_at_epoch, Some(epochs.reset_at));
    assert_eq!(spend.renews_at_epoch, None);
    assert_eq!(plan.kind, UsageMetricGroupKindV1::Plan);
    assert_eq!(plan.reset_at_epoch, None);
    assert_eq!(plan.renews_at_epoch, Some(epochs.renews_at));
}

fn assert_metric_group_meters(account: &UsageAccount) {
    let window = test_metric_group(
        UsageMetricGroupKindV1::Window,
        UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_percent_origin: None,
            used_percent: Some(RawPercentPoints::new(30.0).expect("valid percent")),
            used_percent_origin: Some(PercentObservationOrigin::ProviderReported),
            meter_percent: Some(MeterPercentPoints::new(30.0).expect("valid meter")),
            meter_side: Some(UsageMeterSide::Used),
            count_quota: None,
            provider_quota_state: None,
            money_used_percent: None,
            period: UsageMetricPeriodV1::Unknown,
            unit: None,
        },
    );
    assert_eq!(
        window.meter_percent().map(MeterPercentPoints::get),
        Some(30.0)
    );
    let empty_window = test_metric_group(
        UsageMetricGroupKindV1::Window,
        UsageMetricValueV1::Window {
            remaining_percent: None,
            remaining_percent_origin: None,
            used_percent: None,
            used_percent_origin: None,
            meter_percent: None,
            meter_side: None,
            count_quota: None,
            provider_quota_state: None,
            money_used_percent: None,
            period: UsageMetricPeriodV1::Unknown,
            unit: None,
        },
    );
    assert_eq!(empty_window.meter_percent(), None);
    for group in &account.metric_groups {
        assert_ne!(group.kind, UsageMetricGroupKindV1::Window);
        assert_eq!(group.meter_percent(), None);
    }
}

fn test_metric_group(kind: UsageMetricGroupKindV1, value: UsageMetricValueV1) -> UsageMetricGroup {
    UsageMetricGroup {
        group_id: "test-group".to_owned(),
        rank: 0,
        kind,
        label: "test group".to_owned(),
        scope: UsageMetricScopeV1::default(),
        observed_at_epoch: None,
        fetched_at_epoch: TEST_NOW_EPOCH,
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

fn assert_detail_projection_rows(detail: &str) {
    assert_rows(
        detail,
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
            "120% used (reported)",
            "pace: on pace",
            "Issues",
            "quota degraded (quota_degraded)",
            "provider: provider maintenance (prov_maint)",
        ],
    );
    assert!(
        !detail.contains("remaining"),
        "unexpected remaining:\n{detail}"
    );
    assert!(!detail.contains("tier"), "unexpected tier:\n{detail}");
    assert!(
        !detail.contains("uncapped"),
        "unexpected uncapped:\n{detail}"
    );
    assert_eq!(detail.matches("expires").count(), 1, "{detail}");
    assert_eq!(detail.matches("resets in").count(), 1, "{detail}");
    assert_eq!(detail.matches("renews in").count(), 1, "{detail}");
}

fn assert_overview_projection_rows(overview: &str) {
    assert_rows(
        overview,
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
    assert!(
        !overview.contains("remaining"),
        "unexpected remaining:\n{overview}"
    );
}
