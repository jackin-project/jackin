// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_account_snapshots_use_in_memory_cache() {
    let mut cache = UsageCache::default();
    cache.snapshots.insert(
        canonical_usage_cache_key("codex", Some("OpenAI")),
        CachedUsage {
            view: codex_cached_usage_view(),
        },
    );

    let accounts = cache.account_snapshot_views();

    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].provider, "OpenAI");
    assert_eq!(accounts[0].account_label, "codex@example.com");
    assert_eq!(accounts[0].source, "provider_api");
    assert_eq!(accounts[0].confidence, "authoritative");
    assert_eq!(accounts[0].window_kind, "Session");
    assert_eq!(accounts[0].used_amount, Some(63));
    assert_eq!(accounts[0].used_unit.as_deref(), Some("percent"));
    assert_eq!(accounts[0].limit_amount, Some(100));
    assert_eq!(accounts[0].limit_unit.as_deref(), Some("percent"));
    assert_eq!(accounts[0].fetched_at, 123);
    assert_eq!(accounts[0].status, "fresh");
}

#[test]
fn account_snapshot_rows_preserve_money_units_for_spend_buckets() {
    let mut view = codex_cached_usage_view();
    view.buckets = vec![QuotaBucketView {
        label: "Extra usage".to_owned(),
        used_label: Some("SGD 78.00 of SGD 260.00".to_owned()),
        limit_label: Some("SGD 260.00".to_owned()),
        remaining_percent: Some(70),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Spend),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: Some(Money::new(7_800, "SGD", 2)),
        limit_money: Some(Money::new(26_000, "SGD", 2)),
        severity: UsageSeverity::Normal,
    }];
    let mut snapshots = HashMap::new();
    snapshots.insert("codex".to_owned(), CachedUsage { view });

    let rows = account_snapshot_views_from_cache(&snapshots);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].used_amount, Some(7_800));
    assert_eq!(rows[0].used_unit.as_deref(), Some("SGD"));
    assert_eq!(rows[0].limit_amount, Some(26_000));
    assert_eq!(rows[0].limit_unit.as_deref(), Some("SGD"));
}

#[test]
fn account_snapshot_rows_propagate_view_failure_to_retained_buckets() {
    let mut view = codex_cached_usage_view();
    view.status = UsageSnapshotStatus::Stale;
    view.buckets[0].status = UsageSnapshotStatus::Fresh;
    let mut snapshots = HashMap::new();
    snapshots.insert("codex".to_owned(), CachedUsage { view });

    let rows = account_snapshot_views_from_cache(&snapshots);
    assert_eq!(rows[0].status, "stale");
}

#[test]
fn materialized_usage_accounts_write_normalized_snapshots() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage").join("accounts.json");
    let mut view = FocusedUsageView::unavailable("none", 123);
    view.focused_agent = Some("codex".to_owned());
    view.status_bar_label = "Codex Session: 63% used · 37% left".to_owned();

    write_materialized_usage_accounts(&path, 456, &[&view]).expect("write accounts");

    let body = fs::read_to_string(&path).expect("accounts json");
    let decoded: MaterializedUsageAccounts = serde_json::from_str(&body).expect("decode accounts");
    assert_eq!(decoded.generated_at_epoch, 456);
    assert_eq!(decoded.snapshots.len(), 1);
    assert_eq!(decoded.snapshots[0].focused_agent.as_deref(), Some("codex"));
    assert_eq!(
        decoded.snapshots[0].status_bar_label,
        "Codex Session: 63% used · 37% left"
    );
    let leftovers = fs::read_dir(path.parent().expect("parent"))
        .expect("read usage dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
        .count();
    assert_eq!(leftovers, 0);
}

#[test]
fn status_bar_label_uses_session_and_weekly_remaining() {
    let buckets = vec![
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Session".to_owned(),
            used_label: Some("63% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(37),
            reset_label: None,
            resets_at: None,
            status_slot: Some(StatusSlot::Session),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
        },
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Weekly".to_owned(),
            used_label: Some("90% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(10),
            reset_label: Some("Resets in 3h 52m".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Weekly),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
        },
    ];

    assert_eq!(
        status_bar_label(
            UsageSurface::Codex,
            "alexey@example.com",
            UsageSnapshotStatus::Fresh,
            &buckets
        ),
        "Session 37% · Weekly 10%"
    );
}

#[test]
fn status_bar_reads_session_weekly_slots_from_tags() {
    // The headline reads the semantic slot the provider tagged at
    // construction, not the (free-text) window label — Z.AI's weekly window
    // is "Tokens", MiniMax's is "General · Weekly", Grok tags its billing
    // cycle Weekly with no session. An untagged window (MCP) never reaches
    // the headline.
    let pct = |label: &str, remaining: u8, slot: Option<StatusSlot>| QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(remaining),
        reset_label: None,
        resets_at: None,
        status_slot: slot,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
    };

    let zai = vec![
        pct("5-hour", 80, Some(StatusSlot::Session)),
        pct("Tokens", 42, Some(StatusSlot::Weekly)),
        pct("MCP", 90, None),
    ];
    assert_eq!(
        status_bar_label(UsageSurface::Zai, "", UsageSnapshotStatus::Fresh, &zai),
        "Session 80% · Weekly 42%"
    );

    let minimax = vec![
        pct("General · 5h", 70, Some(StatusSlot::Session)),
        pct("General · Weekly", 55, Some(StatusSlot::Weekly)),
    ];
    assert_eq!(
        status_bar_label(
            UsageSurface::Minimax,
            "",
            UsageSnapshotStatus::Fresh,
            &minimax
        ),
        "Session 70% · Weekly 55%"
    );

    // Grok: billing cycle tagged Weekly, no session → "Weekly N%".
    let grok = vec![pct("Monthly", 33, Some(StatusSlot::Weekly))];
    assert_eq!(
        status_bar_label(UsageSurface::Grok, "", UsageSnapshotStatus::Fresh, &grok),
        "Weekly 33%"
    );
}

#[test]
fn codex_plan_display_name_matches_codexbar() {
    // ported from CodexBar's CodexPlanFormatting tests.
    assert_eq!(codex_plan_display_name("pro").as_deref(), Some("Pro 20x"));
    assert_eq!(codex_plan_display_name("Pro").as_deref(), Some("Pro 20x"));
    assert_eq!(
        codex_plan_display_name("Codex Pro").as_deref(),
        Some("Pro 20x")
    );
    assert_eq!(
        codex_plan_display_name("prolite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(
        codex_plan_display_name("pro_lite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(
        codex_plan_display_name("Pro Lite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(
        codex_plan_display_name("Codex Pro Lite").as_deref(),
        Some("Pro 5x")
    );
    assert_eq!(codex_plan_display_name(""), None);
    assert_eq!(codex_plan_display_name("   "), None);
    assert_eq!(
        codex_plan_display_name("enterprise_cbp_usage_based").as_deref(),
        Some("Enterprise CBP Usage Based")
    );
    assert_eq!(codex_plan_display_name("k12").as_deref(), Some("K12"));
    assert_eq!(
        codex_plan_display_name("Enterprise").as_deref(),
        Some("Enterprise")
    );
}

#[test]
fn status_bar_label_uses_stale_cached_percentages() {
    let buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Session".to_owned(),
        used_label: Some("99% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(1),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Stale,
    }];

    assert_eq!(
        status_bar_label(
            UsageSurface::Claude,
            "alexey@example.com",
            UsageSnapshotStatus::Stale,
            &buckets
        ),
        "Session 1%"
    );
}

#[test]
fn status_bar_label_drops_tagged_bucket_that_failed() {
    // A Session-tagged bucket whose own status is not Fresh/Stale (e.g. the
    // window errored) must not surface its percentage as if it were live;
    // the headline falls through to the snapshot-level status label.
    let buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Session".to_owned(),
        used_label: Some("50% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(50),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Error,
    }];

    assert_eq!(
        status_bar_label(
            UsageSurface::Claude,
            "alexey@example.com",
            UsageSnapshotStatus::Error,
            &buckets
        ),
        "error"
    );
}

#[test]
fn status_bar_label_uses_amp_daily_only() {
    let buckets = vec![
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Amp Free".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(48),
            reset_label: Some("Resets daily".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Daily),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
        },
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Individual credits".to_owned(),
            used_label: None,
            limit_label: Some("$4.76".to_owned()),
            remaining_percent: None,
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: Some("Individual credits: $4.76".to_owned()),
            status: UsageSnapshotStatus::Fresh,
        },
    ];

    // Daily is the only glance; credits stay detail-only.
    assert_eq!(
        status_bar_label(
            UsageSurface::Amp,
            "alexey@example.com",
            UsageSnapshotStatus::Fresh,
            &buckets
        ),
        "Free 48%"
    );
}

#[test]
fn status_bar_label_uses_stale_amp_cache() {
    let buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Amp Free".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(9),
        reset_label: Some("Resets daily".to_owned()),
        resets_at: None,
        status_slot: Some(StatusSlot::Daily),
        pace_label: None,
        status: UsageSnapshotStatus::Stale,
    }];

    assert_eq!(
        status_bar_label(
            UsageSurface::Amp,
            "alexey@example.com",
            UsageSnapshotStatus::Stale,
            &buckets
        ),
        "Free 9%"
    );
}

#[test]
fn usage_cache_key_canonicalizes_provider_aliases() {
    assert_eq!(
        canonical_usage_cache_key("claude", Some("Anthropic")),
        canonical_usage_cache_key("claude", Some("Anthropic / Claude"))
    );
    assert_eq!(
        canonical_usage_cache_key("codex", Some("OpenAI")),
        canonical_usage_cache_key("codex", Some("OpenAI / Codex"))
    );
    assert_eq!(
        canonical_usage_cache_key("claude", Some("Z.AI")),
        canonical_usage_cache_key("glm", Some("GLM / Z.AI"))
    );
    assert_eq!(
        canonical_usage_cache_key("opencode", Some("OpenRouter")),
        "OpenRouter"
    );
    assert_ne!(
        canonical_usage_cache_key("claude", Some("Anthropic")),
        canonical_usage_cache_key("claude", Some("Z.AI"))
    );
}
