// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parity_antigravity_capsule_tabs_and_detail() {
    let (_, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });

    // Capsule side: one tab, four bucket rows, same percents/resets.
    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs.len(), 1);
    assert_eq!(view.tabs[0].label, "Antigravity · pilot@example.test");
    assert!(view.tabs[0].active);
    assert_eq!(view.tabs[0].plan_label.as_deref(), Some("Antigravity Pro"));
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("fresh · managed CLI")
    );
    // First available Rust-ranked limit (D30) wins the tab status: the
    // Weekly long-range window (41%), not the tighter unslotted 12% window
    // and not the provider-first Session window — the same window as the
    // console list summary.
    assert!(
        view.tabs[0].status_label.starts_with("41% left"),
        "tab status traces the ranked weekly window: {}",
        view.tabs[0].status_label
    );
    assert!(
        view.tabs[0].status_label.contains("Resets in 25h"),
        "tab status carries the reset: {}",
        view.tabs[0].status_label
    );

    let detail = usage_detail_presentation(view);
    let row_labels = detail
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        row_labels,
        [
            "Plan",
            "Auth",
            "Gemini · 5h",
            "Gemini · Weekly",
            "Other models · 5h",
            "Other models · Weekly"
        ]
    );
    let bucket_rows = detail
        .rows
        .iter()
        .filter(|row| row.kind == UsageDetailRowKind::Bucket)
        .collect::<Vec<_>>();
    assert_eq!(bucket_rows.len(), 4);
    for (row, percent) in bucket_rows.iter().zip([73_u8, 41, 12, 88]) {
        assert_eq!(row.meter_percent, Some(percent));
        assert!(
            row.display_label.starts_with(&format!("{percent}% left")),
            "bucket display must lead with the percent: {}",
            row.display_label
        );
    }
    // Same reset epoch, renderer-owned formats (see delta test).
    assert!(
        bucket_rows[0].display_label.contains("Resets in 1h 30m"),
        "capsule reset label: {}",
        bucket_rows[0].display_label
    );

    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.provider_title, "Antigravity");
    assert_eq!(identity.account_label, "pilot@example.test");
    assert_eq!(identity.activity_label, "Updated 5m ago");
    assert_eq!(identity.activity_kind, UsageActivityKind::Idle);
}

#[test]
fn parity_duplicate_provider_accounts_stay_distinct() {
    let work = parity_claude_work_account();
    let personal = parity_claude_personal_account();
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![work.clone(), personal.clone()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 2);
    // Projection order is preserved; stable ids never collide.
    assert_eq!(screen.accounts[0].account, "work@example.test");
    assert_eq!(screen.accounts[1].account, "personal@example.test");
    assert_ne!(
        screen.accounts[0].stable_id(),
        screen.accounts[1].stable_id()
    );
    assert_ne!(
        screen.accounts[0].canonical_account_id,
        screen.accounts[1].canonical_account_id
    );
    // Identity evidence kinds survive: provider id vs stable handle.
    assert_eq!(
        screen.accounts[0].identity_kind,
        Some(UsageIdentityKindV1::ProviderAccountId)
    );
    assert_eq!(
        screen.accounts[1].identity_kind,
        Some(UsageIdentityKindV1::ProviderStableHandle)
    );

    // Capsule tabs: one per account, distinct ids, exact-id active marking.
    let enriched = parity_tabs(&views);
    let tabs = &enriched[0].tabs;
    assert_eq!(tabs.len(), 2);
    let ids = tabs
        .iter()
        .map(|tab| tab.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), 2, "tab ids must be distinct");
    assert_eq!(tabs[0].label, "Anthropic · personal@example.test");
    assert_eq!(tabs[1].label, "Anthropic · work@example.test");
    assert_eq!(
        enriched[0]
            .tabs
            .iter()
            .filter(|tab| tab.active)
            .map(|tab| tab.account_label.as_str())
            .collect::<Vec<_>>(),
        ["work@example.test"]
    );
    assert_eq!(
        enriched[1]
            .tabs
            .iter()
            .filter(|tab| tab.active)
            .map(|tab| tab.account_label.as_str())
            .collect::<Vec<_>>(),
        ["personal@example.test"]
    );

    // Duplicate snapshots for one account collapse to the newest fetch.
    let mut stale_work = parity_view(&work);
    stale_work.fetched_at_epoch = PARITY_NOW - 9_999;
    stale_work.account.plan_label = Some("Max OLD".to_owned());
    let collapsed = provider_tabs(&[&stale_work, &views[0], &views[1]]);
    assert_eq!(collapsed.len(), 2);
    let work_tab = collapsed
        .iter()
        .find(|tab| tab.account_label == "work@example.test")
        .unwrap();
    assert_eq!(work_tab.plan_label.as_deref(), Some("Max 20x"));

    // Legacy provider-label remap is shared and stable.
    assert_eq!(provider_display_label("Anthropic / Claude"), "Anthropic");
    assert_eq!(provider_display_label("Anthropic"), "Anthropic");
}

#[test]
fn parity_cursor_groups_money_and_units() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    let labels = account
        .windows
        .iter()
        .map(|window| window.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        ["Billing cycle", "Spend (actual)", "Credits", "Requests"]
    );
    let meters = account
        .windows
        .iter()
        .map(UsageWindow::meter_percent)
        .collect::<Vec<_>>();
    assert_eq!(meters, [Some(62), Some(55), None, Some(76)]);
    // API-mirrored Warn severity becomes a Warning quota state.
    assert_eq!(account.windows[0].quota_state, UsageQuotaStateV1::Warning);
    assert_eq!(
        account.windows[0].reset_at_epoch,
        Some(PARITY_NOW + 1_200_000)
    );
    // Used-only money still counts as quantity (Available), with no bar.
    assert_eq!(account.windows[2].quota_state, UsageQuotaStateV1::Available);
    assert_eq!(account.windows[2].value, "$8.30");

    // Groups: window, window, spend, window, spend, window, plan, rate-limit.
    let kinds = account
        .metric_groups
        .iter()
        .map(|group| group.kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        [
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::SpendCap,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::SpendCap,
            UsageMetricGroupKindV1::Window,
            UsageMetricGroupKindV1::Plan,
            UsageMetricGroupKindV1::RateLimit,
        ]
    );
    // Spend amounts stay in minor-unit scale end to end ($45.20, not $4520).
    match &account.metric_groups[2].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(
                cap.as_ref().map(ToString::to_string).as_deref(),
                Some("$100.00")
            );
            assert_eq!(
                spent.as_ref().map(ToString::to_string).as_deref(),
                Some("$45.20")
            );
            assert_eq!(
                remaining.as_ref().map(ToString::to_string).as_deref(),
                Some("$54.80")
            );
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }
    // Balance-shaped money (used-only) yields a cap-less spend group
    // (documented delta: renders as "uncapped" — see the render smoke test).
    match &account.metric_groups[4].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(cap, &None);
            assert_eq!(spent.as_ref(), Some(&usd(830)));
            assert_eq!(remaining, &None);
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }
    match &account.metric_groups[7].value {
        UsageMetricValueV1::RateLimit {
            limit,
            remaining,
            window_label,
        } => {
            assert_eq!(*limit, Some(100));
            assert_eq!(*remaining, Some(20));
            assert_eq!(window_label.as_deref(), Some("per minute"));
        }
        other => panic!("expected rate-limit value, got {other:?}"),
    }
    assert_eq!(
        account.metric_groups[7].reset_at_epoch,
        Some(PARITY_NOW + 90)
    );
    assert_eq!(freshness_age_label(PARITY_NOW, account), "updated 10m ago");

    // Capsule side: same amounts, same units.
    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    let spend = usage_bucket_presentation(&view.buckets[1]);
    assert!(
        spend.display_label.contains("$45.20"),
        "{}",
        spend.display_label
    );
    assert!(
        spend.display_label.contains("$100.00"),
        "{}",
        spend.display_label
    );
    let credits = usage_bucket_presentation(&view.buckets[2]);
    assert_eq!(credits.meter_percent, None);
    assert!(
        credits.display_label.contains("$8.30"),
        "{}",
        credits.display_label
    );
    let requests = usage_bucket_presentation(&view.buckets[3]);
    assert_eq!(requests.meter_percent, Some(76));
    assert!(
        requests.display_label.starts_with("76% left"),
        "{}",
        requests.display_label
    );
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Updated 10m ago");
}

#[test]
fn parity_exhausted_zero_is_not_unknown() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "openai",
        display_name: "OpenAI",
        accounts: vec![parity_exhausted_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    // Fresh account, exhausted window: a legit zero, never unknown.
    assert_eq!(account.status, "Available");
    assert_eq!(account.windows.len(), 1);
    assert_eq!(account.windows[0].meter_percent(), Some(0));
    assert_eq!(account.windows[0].value, "0% left");
    assert_eq!(account.windows[0].quota_state, UsageQuotaStateV1::Exhausted);
    assert_ne!(account.windows[0].quota_state, UsageQuotaStateV1::Unknown);
    // Missing plan label yields no plan group and no invented values.
    assert_eq!(account.metric_groups.len(), 1);
    assert_eq!(account.plan_label, None);

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    let bucket = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(bucket.meter_percent, Some(0));
    assert!(
        bucket.display_label.starts_with("0% left"),
        "{}",
        bucket.display_label
    );
    assert!(
        view.tabs[0].status_label.starts_with("0% left"),
        "{}",
        view.tabs[0].status_label
    );
}
