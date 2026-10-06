// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn documented_delta_balance_value_and_uncapped_spend() {
    // Limit-only balances (the Grok prepaid seam): the console window value
    // is blank (the projection falls back to `used_label` only) while the
    // capsule shows "$5.00" via its balance seam. Used-only money yields a
    // cap-less console spend group ("uncapped · spent $8.30" — pinned in the
    // render smoke test). Disposition: RECORDED — projection-owned
    // (`value_label` fallback; slot-blind spend-group emission); the console
    // cannot recover either from its inputs.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "xai",
        display_name: "xAI",
        accounts: vec![parity_partial_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts[0].windows[1].value, "");
    let credits = usage_bucket_presentation(&views[0].buckets[1]);
    assert!(
        credits.display_label.contains("$5.00"),
        "{}",
        credits.display_label
    );

    let (projection, _) = parity_single_provider(ParityProvider {
        provider_id: "cursor",
        display_name: "Cursor",
        accounts: vec![parity_cursor_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert!(
        matches!(
            &screen.accounts[0].metric_groups[4].value,
            UsageMetricValueV1::SpendCap {
                cap: None,
                spent: Some(_),
                ..
            }
        ),
        "balance-shaped money yields a cap-less spend group"
    );
}

#[test]
fn documented_delta_model_scope_capsule_gap() {
    // Console groups carry model/pool scope ("scope: pool credits-pool" —
    // pinned in the detail render); capsule buckets and detail rows have no
    // scope surface. (Related: the console detail pane has no username row
    // while capsule does — the projection never carries `username`.)
    // Disposition: RECORDED — `QuotaBucketView` lacks scope axes and the
    // projection drops `username`; both need protocol changes.
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "antigravity",
        display_name: "Antigravity",
        accounts: vec![parity_antigravity_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].metric_groups[5].scope.pool.as_deref(),
        Some("credits-pool")
    );
    let detail = usage_detail_presentation(&views[0]);
    assert!(
        detail
            .rows
            .iter()
            .all(|row| !row.display_label.contains("credits-pool")),
        "capsule rows have no scope surface"
    );

    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![parity_claude_work_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(
        screen.accounts[0].metric_groups[3].scope.model.as_deref(),
        Some("claude-opus-4-6")
    );
    let detail = usage_detail_presentation(&views[0]);
    assert!(
        detail
            .rows
            .iter()
            .all(|row| !row.display_label.contains("claude-opus-4-6")),
        "capsule rows have no scope surface"
    );
    assert!(
        detail
            .rows
            .iter()
            .any(|row| row.label == "Username" && row.display_label == "work-user"),
        "capsule keeps the username row the console projection drops"
    );
}

#[test]
fn parity_overage_magnitude_matches_raw_money() {
    // $150 against a $100 cap: both surfaces read raw 150 through the same
    // shared money-ratio rule — console "150% used" (plus the raw note,
    // pinned in the render smoke test) and capsule "150% used" — and both
    // meters read empty (nothing left).
    let (projection, views) = parity_single_provider(ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![parity_claude_personal_account()],
        provider_issues: Vec::new(),
    });
    let screen = UsageScreenState::from_projection(&projection);
    let window = &screen.accounts[0].windows[2];
    assert_eq!(window.used_percent, Some(100));
    assert_eq!(window.used_raw_percent, Some(150));
    assert_eq!(window.remaining_percent, None);
    assert_eq!(window.value, "150% used");
    assert_eq!(window.meter_percent(), Some(0));
    assert_eq!(window.quota_state, UsageQuotaStateV1::Exhausted);
    let spend = usage_bucket_presentation(&views[0].buckets[2]);
    assert_eq!(spend.remaining_label.as_deref(), Some("150% used"));
    assert_eq!(spend.meter_percent, Some(0));
}

#[test]
fn parity_meter_percent_inputs_agree() {
    // Every console window meter equals the capsule bucket meter — spend
    // included, overage included (both read empty at 150% used). Rendered
    // glyphs match statically: the console `meter_line` and the capsule
    // full-width meter both draw `█`/`░` (the capsule's intermediate `·`
    // empty cell never reaches the screen).
    let providers = parity_mega_providers();
    let defs = providers
        .iter()
        .flat_map(|provider| provider.accounts.iter())
        .collect::<Vec<_>>();
    let (projection, views_by_provider) = parity_projection(&providers, Vec::new(), Vec::new());
    let screen = UsageScreenState::from_projection(&projection);
    assert_eq!(screen.accounts.len(), defs.len());
    let views = views_by_provider.iter().flatten().collect::<Vec<_>>();
    for ((def, view), console) in defs.iter().zip(views.iter()).zip(screen.accounts.iter()) {
        assert_eq!(
            console.windows.len(),
            view.buckets.len(),
            "window/bucket count for {}",
            def.account_label
        );
        for ((bucket_def, bucket), window) in def
            .buckets
            .iter()
            .zip(view.buckets.iter())
            .zip(console.windows.iter())
        {
            assert_eq!(
                window.meter_percent(),
                usage_bucket_presentation(bucket).meter_percent,
                "meter agreement for {}",
                bucket_def.label
            );
        }
    }
}

#[test]
fn documented_delta_error_buckets_carry_no_percent() {
    // The quota-driven console color and the severity-driven capsule accent
    // can only disagree on error-status buckets that still carry a percent;
    // no harness error/login/unsupported bucket does (adapters omit percents
    // there). Disposition: RECORDED residual with fixture evidence.
    let providers = parity_mega_providers();
    let (_, views_by_provider) = parity_projection(&providers, Vec::new(), Vec::new());
    for view in views_by_provider.iter().flatten() {
        for bucket in &view.buckets {
            if matches!(
                bucket.status,
                UsageSnapshotStatus::Error
                    | UsageSnapshotStatus::NeedsLogin
                    | UsageSnapshotStatus::NeedsSecret
                    | UsageSnapshotStatus::Unsupported
            ) {
                assert_eq!(
                    bucket.remaining_percent, None,
                    "error-status bucket {} must not carry a percent",
                    bucket.label
                );
            }
        }
    }
    // The auth and error fixtures carry no buckets at all.
    let auth_view = parity_view(&parity_auth_account());
    assert!(auth_view.buckets.is_empty());
    let error_view = parity_view(&parity_error_account());
    assert!(error_view.buckets.is_empty());
}

#[test]
fn parity_console_render_smoke_overview() {
    // Renderer-private strings pinned end to end: every scenario provider,
    // both unresolved rows, the notice, and the projection issue.
    let screen = parity_mega_screen(PARITY_NOW);
    let text = parity_render_text(screen.clone(), 150, 240, PARITY_NOW);
    let repeated = parity_render_text(screen, 150, 240, PARITY_NOW);
    assert_eq!(
        text, repeated,
        "fixed-clock parity render must be repeatable"
    );
    for expected in [
        // Antigravity two-family fixture.
        "Antigravity · pilot@example.test",
        "Gemini · 5h",
        "73% left",
        "Other models · 5h",
        "12% left",
        "Gemini · 5h: 73% left · provider-defined period",
        "Gemini · Weekly: 41% left · weekly",
        "Other models · 5h: 12% left",
        "resets in 1h",
        "resets in 1d",
        "resets in 13d",
        "Plan: Antigravity Pro",
        "Credits: $12.50",
        "expires in 30d",
        // Duplicate provider accounts plus overage.
        "Anthropic · work@example.test",
        "Anthropic · personal@example.test",
        "150% used",
        "raw used 150%",
        // Cursor groups and exact money units.
        "Cursor · cursor-user",
        "Billing cycle",
        "Spend (actual) spend: cap $100.00 · spent $45.20 · remaining $54.80",
        "Credits spend: uncapped · spent $8.30",
        "API rate limit: limit 100 · remaining 20 · per minute",
        "resets in 1m",
        // Legit-zero exhaustion.
        "OpenAI · zero@example.test",
        "0% left",
        "quota: exhausted",
        // Stale partial failure with retry + provider issue + unknown.
        "xAI · partial@example.test",
        "stale · updated 25m ago",
        "rate limited by provider (rate_limited) · retry in 5m",
        "provider: provider responding slowly (provider_slow) · retry in 10m",
        "quota: unknown",
        // Unsupported / auth-expired / hard-error states.
        "MiniMax · mm-user",
        "quota: unsupported",
        "Z.AI · zai-user",
        "Needs login",
        "Credential expired 1h ago",
        "sign in required (auth_required)",
        "Kimi · kimi-user",
        "usage request timed out (timeout) · retry in 2m",
        "usage response malformed (malformed)",
        // Legit-zero spend with missing identity fields.
        "OpenCode · ",
        "100% left",
        "Tokens spend: cap $100.00 · spent $0.00 · remaining $100.00",
        // Unresolved rows, notice, and projection issue.
        "Unresolved (anthropic:key)",
        "needs login · authentication required",
        "Unresolved (openai:second)",
        "2 configured capability(s) unresolved",
        "one provider refresh failed (broker_degraded)",
    ] {
        assert!(
            text.contains(expected),
            "overview render must contain {expected:?}:\n{text}"
        );
    }
}

#[test]
fn parity_console_render_smoke_detail_scopes() {
    // Group scope lines only render in the account detail pane.
    let mut screen = parity_mega_screen(PARITY_NOW);
    screen.selected = 1;
    screen.detail = true;
    let text = parity_render_text(screen, 120, 70, PARITY_NOW);
    for expected in [
        "Provider  Antigravity",
        "Account   pilot@example.test",
        "Status    Available",
        "Plan      Antigravity Pro",
        "Identity  provider handle",
        "Freshness updated 5m ago",
        "73% left · Resets in 1h 30m",
        "pace: On pace",
        "Credits (balance · available · updated 5m ago)",
        "scope: pool credits-pool",
        "expires in 30d",
        "fetched 5m ago",
    ] {
        assert!(
            text.contains(expected),
            "antigravity detail must contain {expected:?}:\n{text}"
        );
    }

    let mut screen = parity_mega_screen(PARITY_NOW);
    screen.selected = 2;
    screen.detail = true;
    let text = parity_render_text(screen, 120, 70, PARITY_NOW);
    for expected in [
        "Tokens (token totals · n/a · updated 2m ago)",
        "scope: model claude-opus-4-6",
        "input 1500000 · output 320000 · cached 900000 · this week",
        "Max 20x",
    ] {
        assert!(
            text.contains(expected),
            "work detail must contain {expected:?}:\n{text}"
        );
    }
    // The projection never carries `username`: the console detail pane has
    // no username row while the capsule keeps one (protocol gap, noted in
    // the parity report).
    assert!(
        !text.contains("work-user"),
        "console detail must not invent a username row:\n{text}"
    );
}

#[test]
fn parity_s5_render_smoke() {
    let providers = [ParityProvider {
        provider_id: "anthropic",
        display_name: "Anthropic",
        accounts: vec![parity_claude_work_account(), parity_old_stale_account()],
        provider_issues: Vec::new(),
    }];
    let (projection, _) = parity_projection_at(
        PARITY_NOW,
        &providers,
        Vec::new(),
        vec![parity_projection_issue()],
    );
    let screen = UsageScreenState::from_projection(&projection);
    let text = parity_render_text(screen, 120, 50, PARITY_NOW);
    for expected in [
        "Anthropic · work@example.test",
        "updated 2m ago",
        "Anthropic · old@example.test",
        "stale · updated 1d ago",
        "one provider refresh failed (broker_degraded)",
    ] {
        assert!(
            text.contains(expected),
            "S5 render must contain {expected:?}:\n{text}"
        );
    }
}
