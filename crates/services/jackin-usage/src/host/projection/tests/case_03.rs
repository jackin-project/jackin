// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parity_stale_partial_windows_issues_and_retry() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    // Stale but Available: the status override names staleness honestly.
    assert_eq!(account.status, "stale");
    assert!(account.is_stale);
    assert_eq!(account.freshness_phase, UsageFreshnessPhaseV1::Stale);
    assert_eq!(
        freshness_age_label(PARITY_NOW, account),
        "stale · updated 25m ago"
    );
    // Retained last-good windows survive the partial failure.
    assert_eq!(account.windows.len(), 3);
    assert_eq!(account.windows[0].meter_percent(), Some(54));
    assert_eq!(account.windows[0].reset_at_epoch, Some(PARITY_NOW + 70_000));
    // Limit-only balance: usable quantity, but no percent means no bar.
    assert_eq!(account.windows[1].quota_state, UsageQuotaStateV1::Available);
    assert_eq!(account.windows[1].meter_percent(), None);
    // Quantity-less window: unknown, never a fabricated 0% bar.
    assert_eq!(account.windows[2].quota_state, UsageQuotaStateV1::Unknown);
    assert_eq!(account.windows[2].meter_percent(), None);
    // Freshness is per group: every group is stale with the same age.
    for group in &account.metric_groups {
        assert_eq!(
            group_freshness_label(PARITY_NOW, group),
            "stale · updated 25m ago"
        );
    }
    // The rate-limit issue keeps its code and broker retry.
    assert_eq!(account.issues.len(), 1);
    assert_eq!(account.issues[0].code, "rate_limited");
    assert_eq!(account.issues[0].retry_at_epoch, Some(PARITY_NOW + 330));
    assert_eq!(account.issue_count(), 1);

    // Capsule side: same windows, same staleness, degraded gracefully.
    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "stale");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("stale · provider")
    );
    let weekly = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(weekly.meter_percent, Some(54));
    assert!(
        weekly.display_label.contains("54% left"),
        "{}",
        weekly.display_label
    );
    assert!(
        weekly.display_label.contains("stale"),
        "{}",
        weekly.display_label
    );
    let credits = usage_bucket_presentation(&view.buckets[1]);
    assert_eq!(credits.meter_percent, None);
    assert!(
        credits.display_label.contains("$5.00"),
        "{}",
        credits.display_label
    );
    let unknown = usage_bucket_presentation(&view.buckets[2]);
    assert_eq!(unknown.meter_percent, None);
    assert!(
        !unknown.display_label.contains("0%"),
        "{}",
        unknown.display_label
    );
    let detail = usage_detail_presentation(view);
    let last = detail.rows.last().unwrap();
    assert_eq!(last.label, "Detail");
    assert_eq!(
        last.display_label,
        "rate limited by provider; showing last cached quota"
    );
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Update delayed · Updated 25m ago");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_unsupported_lifecycle_words() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "minimax",
        display_name: "MiniMax",
        accounts: vec![parity_unsupported_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    assert_eq!(account.lifecycle, UsageLifecycleV1::Unsupported);
    // Projection-owned status word (capitalized — see the wording delta).
    assert_eq!(account.status, "Unsupported");
    assert_eq!(account.freshness_phase, UsageFreshnessPhaseV1::Failed);
    assert_eq!(freshness_age_label(PARITY_NOW, account), "never updated");
    assert_eq!(account.windows.len(), 1);
    assert_eq!(
        account.windows[0].quota_state,
        UsageQuotaStateV1::Unsupported
    );
    assert_eq!(account.windows[0].meter_percent(), None);

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "unsupported");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("unsupported · no source")
    );
    let bucket = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(bucket.meter_percent, None);
    assert_eq!(bucket.display_label, "unsupported");
    let detail = usage_detail_presentation(view);
    let row_labels = detail
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(row_labels, ["Quota", "Detail"]);
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Usage limits unsupported");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_auth_expired_login_state() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "zai",
        display_name: "Z.AI",
        accounts: vec![parity_auth_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    assert_eq!(account.lifecycle, UsageLifecycleV1::NeedsLogin);
    assert_eq!(account.status, "Needs login");
    assert!(account.windows.is_empty());
    assert!(account.metric_groups.is_empty());
    assert_eq!(
        account.credential_expires_at_epoch,
        Some(PARITY_NOW - 3_600)
    );
    assert_eq!(account.issues.len(), 1);
    assert_eq!(account.issues[0].code, "auth_required");
    // Source-capability subjects map to the stable-handle evidence kind.
    assert_eq!(
        account.identity_kind,
        Some(UsageIdentityKindV1::ProviderStableHandle)
    );

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "needs login");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("needs login · no source")
    );
    // No buckets, no identity extras: only the error detail row remains.
    let detail = usage_detail_presentation(view);
    assert_eq!(detail.rows.len(), 1);
    assert_eq!(detail.rows[0].label, "Detail");
    assert_eq!(detail.rows[0].display_label, "sign in required");
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Sign in required");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_error_timeout_and_malformed() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "kimi",
        display_name: "Kimi",
        accounts: vec![parity_error_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    assert_eq!(account.lifecycle, UsageLifecycleV1::Error);
    assert_eq!(account.status, "Error");
    assert_eq!(freshness_age_label(PARITY_NOW, account), "never updated");
    assert_eq!(account.issue_count(), 2);
    assert_eq!(account.issues[0].code, "timeout");
    assert_eq!(account.issues[0].retry_at_epoch, Some(PARITY_NOW + 150));
    assert_eq!(account.issues[1].code, "malformed");
    assert_eq!(account.issues[1].retry_at_epoch, None);

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    assert_eq!(view.tabs[0].status_label, "error");
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("error · no source")
    );
    let detail = usage_detail_presentation(view);
    let last = detail.rows.last().unwrap();
    assert_eq!(last.display_label, "usage request timed out");
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.activity_label, "Update failed · Error");
    assert_eq!(identity.activity_kind, UsageActivityKind::Exceptional);
}

#[test]
fn parity_legit_zero_spend_and_missing_fields() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "opencode",
        display_name: "OpenCode",
        accounts: vec![parity_zero_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let account = &screen.accounts[0];
    // The empty display label is preserved, never replaced.
    assert_eq!(account.account, "");
    assert_eq!(account.plan_label, None);
    assert_eq!(account.windows.len(), 1);
    assert_eq!(account.windows[0].meter_percent(), Some(100));
    assert_eq!(account.windows[0].value, "100% left");
    // Zero spend is tracked data: cap, spent, and remaining all survive.
    assert_eq!(account.metric_groups.len(), 2);
    match &account.metric_groups[1].value {
        UsageMetricValueV1::SpendCap {
            cap,
            spent,
            remaining,
        } => {
            assert_eq!(spent.as_ref(), Some(&usd(0)));
            assert_eq!(cap.as_ref(), Some(&usd(10_000)));
            assert_eq!(remaining.as_ref(), Some(&usd(10_000)));
        }
        other => panic!("expected spend-cap value, got {other:?}"),
    }

    let enriched = parity_tabs(&views);
    let view = &enriched[0];
    // Empty identity degrades honestly: bare provider tab, no invented name.
    assert_eq!(view.tabs[0].label, "OpenCode");
    assert_eq!(view.tabs[0].account_label, "account unavailable");
    assert!(view.tabs[0].active);
    assert_eq!(
        view.tabs[0].source_label.as_deref(),
        Some("fresh · local estimate")
    );
    let spend = usage_bucket_presentation(&view.buckets[0]);
    assert_eq!(spend.remaining_label.as_deref(), Some("0% used"));
    assert!(
        spend.display_label.contains("$0.00"),
        "{}",
        spend.display_label
    );
    assert!(
        spend.display_label.contains("$100.00"),
        "{}",
        spend.display_label
    );
    // No username/plan/auth/error: a single bucket row, nothing fabricated.
    let detail = usage_detail_presentation(view);
    assert_eq!(detail.rows.len(), 1);
    assert_eq!(detail.rows[0].kind, UsageDetailRowKind::Bucket);
    let identity = usage_identity_presentation(
        provider_display_label(&view.account.provider_label),
        view,
        false,
    );
    assert_eq!(identity.account_label, "No authenticated account");
    assert_eq!(identity.activity_label, "Updated now");
    assert_eq!(identity.activity_kind, UsageActivityKind::Idle);
}

#[test]
fn parity_s4_empty_inventory() {
    let (projection, views_by_provider) = parity_projection(&[], Vec::new(), Vec::new());
    assert!(views_by_provider.is_empty());
    let screen = UsageScreenState::from_projection(&projection);
    assert!(screen.accounts.is_empty());
    assert_eq!(screen.notice, None);
    assert_eq!(screen.generated_at_epoch, Some(PARITY_NOW));
    assert!(screen.projection_issues.is_empty());

    // Empty scope stays empty: no tabs, no rows, no invented providers.
    assert!(provider_tabs(&[]).is_empty());
    assert!(parity_tabs(&[]).is_empty());

    let text = parity_render_text(screen, 100, 24, PARITY_NOW);
    assert!(
        text.contains("No providers configured"),
        "empty inventory must explain itself:\n{text}"
    );
}

#[test]
fn parity_s5_stale_and_partial_failure() {
    let (projection, views_by_provider) = parity_projection(
        &[ParityProvider {
            provider_id: "anthropic",
            display_name: "Anthropic",
            accounts: vec![parity_claude_work_account(), parity_old_stale_account()],
            provider_issues: Vec::new(),
        }],
        Vec::new(),
        vec![parity_projection_issue()],
    );
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 2);
    // Fresh and stale siblings keep independent freshness: neither clears
    // the other, and the projection issue does not rewrite either status.
    let fresh = &screen.accounts[0];
    assert_eq!(fresh.status, "Available");
    assert!(!fresh.is_stale);
    assert_eq!(freshness_age_label(PARITY_NOW, fresh), "updated 2m ago");
    assert!(
        fresh
            .metric_groups
            .iter()
            .all(|group| !group.is_stale && group.phase == UsageFreshnessPhaseV1::Current)
    );
    let stale = &screen.accounts[1];
    assert_eq!(stale.status, "stale");
    assert!(stale.is_stale);
    assert_eq!(
        freshness_age_label(PARITY_NOW, stale),
        "stale · updated 1d ago"
    );
    assert!(
        stale
            .metric_groups
            .iter()
            .all(|group| group.is_stale && group.phase == UsageFreshnessPhaseV1::Stale)
    );
    assert_eq!(screen.projection_issues.len(), 1);
    assert_eq!(screen.projection_issues[0].code, "broker_degraded");

    let views = views_by_provider.into_iter().next().unwrap();
    let enriched = parity_tabs(&views);
    assert_eq!(enriched.len(), 2);
    let fresh_tabs = &enriched[0].tabs;
    assert_eq!(fresh_tabs.len(), 2);
    let stale_tab = fresh_tabs
        .iter()
        .find(|tab| tab.account_label == "old@example.test")
        .unwrap();
    assert_eq!(stale_tab.status_label, "stale");
    assert_eq!(stale_tab.source_label.as_deref(), Some("stale · provider"));
    let work_tab = fresh_tabs
        .iter()
        .find(|tab| tab.account_label == "work@example.test")
        .unwrap();
    assert!(
        work_tab.status_label.contains("% left"),
        "{}",
        work_tab.status_label
    );
    assert_eq!(work_tab.source_label.as_deref(), Some("fresh · provider"));
    let stale_identity = usage_identity_presentation(
        provider_display_label(&enriched[1].account.provider_label),
        &enriched[1],
        false,
    );
    assert_eq!(
        stale_identity.activity_label,
        "Update delayed · Updated 25h ago"
    );
}
