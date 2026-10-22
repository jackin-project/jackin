// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn parity_unresolved_stays_console_only() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "openai",
        display_name: "OpenAI",
        accounts: vec![parity_exhausted_account()],
        provider_issues: Vec::new(),
    });
    let mut projection = projection;
    projection.unresolved = parity_unresolved_entries();
    projection.validate().unwrap();
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), 3);
    assert_eq!(screen.accounts[0].provider, "OpenAI");
    assert_eq!(screen.accounts[0].account, "zero@example.test");
    assert!(!screen.accounts[0].unresolved);
    assert_eq!(screen.accounts[1].provider, "OpenAI");
    assert_eq!(screen.accounts[1].account, "Unresolved (openai:second)");
    assert_eq!(screen.accounts[1].status, "needs login");
    assert!(screen.accounts[1].unresolved);
    assert_eq!(screen.accounts[1].stable_id(), "openai:openai:second");
    assert_eq!(screen.accounts[2].provider, "Anthropic");
    assert_eq!(screen.accounts[2].account, "Unresolved (anthropic:key)");
    assert_eq!(
        screen.accounts[2].status,
        "needs login · authentication required"
    );
    assert!(screen.accounts[2].unresolved);
    assert_eq!(screen.accounts[2].identity_kind, None);
    assert_eq!(
        screen.notice.as_deref(),
        Some("2 configured capability(s) unresolved")
    );

    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs.len(), 1);
    assert_eq!(enriched[0].tabs[0].account_label, "zero@example.test");
}

#[test]
fn parity_overview_summary_first_ranked_limit() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let summary = screen.accounts[0]
        .summary_window()
        .expect("a ranked summary window");
    assert_eq!(summary.label, "Gemini · Weekly");
    assert_eq!(
        summary.meter_percent().map(MeterPercentPoints::get),
        Some(41.0)
    );
    let enriched = parity_tabs(&views);
    assert!(
        enriched[0].tabs[0].status_label.starts_with("41% left"),
        "capsule summary traces the same ranked window: {}",
        enriched[0].tabs[0].status_label
    );
}

#[test]
fn parity_spend_meter_uses_authoritative_money_geometry() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let spend_window = &screen.accounts[0].windows[1];
    assert_eq!(
        spend_window.meter_percent().map(MeterPercentPoints::get),
        Some(45.2)
    );
    assert_eq!(spend_window.meter_side, Some(UsageMeterSide::Used));
    assert_eq!(
        spend_window.remaining_percent.map(RawPercentPoints::get),
        Some(55.0)
    );
    assert_eq!(
        spend_window.remaining_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );
    let spend = usage_bucket_presentation(&views[0].buckets[1]);
    assert_eq!(spend.meter_percent.map(MeterPercentPoints::get), Some(45.2));
    assert_eq!(spend.meter_side, Some(UsageMeterSide::Used));
    assert_eq!(spend.remaining_label.as_deref(), Some("45.2% used"));
    assert!(spend.display_label.contains("45.2% used"));
    assert!(spend.display_label.contains("55% left"));

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "opencode",
        display_name: "OpenCode",
        accounts: vec![parity_zero_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let zero_window = &screen.accounts[0].windows[0];
    assert_eq!(
        zero_window.meter_percent().map(MeterPercentPoints::get),
        Some(0.0)
    );
    assert_eq!(zero_window.meter_side, Some(UsageMeterSide::Used));
    assert_eq!(
        zero_window.remaining_percent.map(RawPercentPoints::get),
        Some(100.0)
    );
    assert_eq!(
        zero_window.remaining_percent_origin,
        Some(PercentObservationOrigin::Derived)
    );
    let zero_bucket = usage_bucket_presentation(&views[0].buckets[0]);
    assert_eq!(
        zero_bucket.meter_percent,
        Some(MeterPercentPoints::new(0.0).expect("bounded meter points"))
    );
    assert_eq!(zero_bucket.meter_side, Some(UsageMeterSide::Used));
    assert_eq!(zero_bucket.remaining_label.as_deref(), Some("0% used"));
    assert!(zero_bucket.display_label.contains("100% left"));
}

#[test]
fn documented_delta_severity_color_inputs() {
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![parity_claude_personal_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[1].quota_state,
        UsageQuotaStateV1::Exhausted,
        "Danger severity must surface as Exhausted"
    );
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[0].quota_state,
        UsageQuotaStateV1::Warning,
        "Warn severity must surface as Warning"
    );
    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].windows[2].quota_state,
        UsageQuotaStateV1::Available,
        "Normal severity stays Available even at 12% (both renderers green)"
    );
}

#[test]
fn documented_delta_reset_and_freshness_wording() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let console_window = &screen.accounts[0].windows[0];
    let capsule_bucket = &views[0].buckets[0];
    assert_eq!(console_window.reset_at_epoch, capsule_bucket.resets_at);
    assert_eq!(console_window.reset_at_epoch, Some(PARITY_NOW + 5_430));
    assert_eq!(
        console_window.reset,
        capsule_bucket.reset_label.clone().unwrap()
    );
    assert!(
        capsule_bucket
            .reset_label
            .clone()
            .unwrap()
            .starts_with("Resets in 1h 30m"),
        "capsule countdown form: {:?}",
        capsule_bucket.reset_label
    );
    assert_eq!(
        freshness_age_label(PARITY_NOW, &screen.accounts[0]),
        "updated 5m ago"
    );
    assert_eq!(views[0].updated_label, "Updated 5m ago");

    let (projection, views_by_provider) = parity_projection(
        &[ParityProvider {
            provider_id: "anthropic",
            display_name: "Anthropic",
            accounts: vec![parity_old_stale_account()],
            provider_issues: Vec::new(),
        }],
        Vec::new(),
        Vec::new(),
    );
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        freshness_age_label(PARITY_NOW, &screen.accounts[0]),
        "stale · updated 1d ago"
    );
    let views = views_by_provider.into_iter().next().unwrap();
    assert_eq!(views[0].updated_label, "Updated 25h ago");
}

#[test]
fn documented_delta_status_wording() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "openai",
        display_name: "OpenAI",
        accounts: vec![parity_exhausted_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Available");
    let enriched = parity_tabs(&views);
    assert_eq!(
        enriched[0].tabs[0].source_label.as_deref(),
        Some("fresh · provider")
    );

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "minimax",
        display_name: "MiniMax",
        accounts: vec![parity_unsupported_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Unsupported");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "unsupported");

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "zai",
        display_name: "Z.AI",
        accounts: vec![parity_auth_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Needs login");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "needs login");

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "kimi",
        display_name: "Kimi",
        accounts: vec![parity_error_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "Error");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "error");

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].status, "stale");
    let enriched = parity_tabs(&views);
    assert_eq!(enriched[0].tabs[0].status_label, "stale");
}

#[test]
fn documented_delta_credential_expiry_capsule_gap() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "zai",
        display_name: "Z.AI",
        accounts: vec![parity_auth_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].credential_expires_at_epoch,
        Some(PARITY_NOW - 3_600)
    );
    let enriched = parity_tabs(&views);
    let identity = usage_identity_presentation(
        provider_display_label(&enriched[0].account.provider_label),
        &enriched[0],
        false,
    );
    assert_eq!(identity.activity_label, "Sign in required");
    assert!(
        !format!("{identity:?}").contains("expir"),
        "capsule identity must not invent expiry text: {identity:?}"
    );
}

#[test]
fn documented_delta_issue_retry_capsule_gap() {
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].issues[0].code, "rate_limited");
    assert_eq!(
        screen.accounts[0].issues[0].retry_at_epoch,
        Some(PARITY_NOW + 330)
    );
    let detail = usage_detail_presentation(&views[0]);
    let last = detail.rows.last().unwrap();
    assert_eq!(
        last.display_label,
        "rate limited by provider; showing last cached quota"
    );
    assert!(
        !last.display_label.contains("retry"),
        "{}",
        last.display_label
    );

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "kimi",
        display_name: "Kimi",
        accounts: vec![parity_error_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].issues[0].code, "timeout");
    assert_eq!(
        screen.accounts[0].issues[0].retry_at_epoch,
        Some(PARITY_NOW + 150)
    );
    let detail = usage_detail_presentation(&views[0]);
    assert_eq!(
        detail.rows.last().unwrap().display_label,
        "usage request timed out"
    );
}
