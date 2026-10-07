// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn broker_account_id_for_tab_id_recovers_broker_key() {
    use jackin_protocol::usage_broker::UsageAccountCapability;

    let mut cache = UsageCache::default();
    let capability_a = UsageAccountCapability {
        account_id: "broker-claude-a".to_owned(),
        surface_id: "claude".to_owned(),
    };
    cache.insert_snapshot_for_capability_for_test(
        "claude",
        Some("Anthropic"),
        &capability_a,
        account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100),
    );
    cache.insert_snapshot_for_test(
        "codex",
        Some("OpenAI"),
        account_snapshot_view("OpenAI", "codex@example.com", Some("Pro 20x"), 150),
    );

    assert_eq!(
        cache.broker_account_id_for_tab_id(&usage_account_tab_id("Anthropic", "a@example.com")),
        Some("broker-claude-a".to_owned())
    );
    // Legacy keys carry no capability; unknown ids match nothing.
    assert_eq!(
        cache.broker_account_id_for_tab_id(&usage_account_tab_id("OpenAI", "codex@example.com")),
        None
    );
    assert_eq!(cache.broker_account_id_for_tab_id("sha256:unknown"), None);
}

#[test]
fn usage_status_label_reads_in_memory_cache() {
    let mut cache = UsageCache::default();
    let view = codex_cached_usage_view();
    let expected = view.status_bar_label.clone();
    cache.snapshots.insert(
        canonical_usage_cache_key("codex", Some("OpenAI")),
        CachedUsage { view },
    );

    assert_eq!(
        cache.focused_status_bar_label(Some("codex"), Some("OpenAI")),
        Some(expected)
    );
}

#[test]
fn usage_snapshot_reads_in_memory_cache() {
    let mut cache = UsageCache::default();
    let view = codex_cached_usage_view();
    let expected_label = view.status_bar_label.clone();
    cache.snapshots.insert(
        canonical_usage_cache_key("codex", Some("OpenAI")),
        CachedUsage { view },
    );

    let snapshot = cache.focused_snapshot(Some("codex"), Some("OpenAI"));

    assert_eq!(snapshot.status_bar_label, expected_label);
    assert_eq!(snapshot.account.account_label, "codex@example.com");
    assert!(
        snapshot
            .tabs
            .iter()
            .any(|tab| tab.label == "OpenAI · codex@example.com" && tab.active)
    );
}

#[test]
fn usage_status_label_cache_miss_is_refreshing() {
    let cache = UsageCache::default();

    // A focused agent with no cached snapshot is mid-load → `refreshing`
    // (P3), computed without touching the store.
    assert_eq!(
        cache.focused_status_bar_label(Some("codex"), Some("OpenAI")),
        Some("refreshing".to_owned())
    );
}

#[test]
fn usage_snapshot_cache_miss_is_refreshing() {
    let mut cache = UsageCache::default();

    let snapshot = cache.focused_snapshot(Some("codex"), Some("OpenAI"));

    // a focused agent with no cached snapshot renders `refreshing`
    // (still without reading the store), not a stale/unavailable headline.
    assert_eq!(snapshot.status_bar_label, "refreshing");
    assert_eq!(snapshot.last_error.as_deref(), Some("refreshing"));
}

#[test]
fn focused_usage_lifecycle_hides_before_start_and_refreshes_on_start() {
    // P3 lifecycle: no focused agent → segment hidden; focused agent with
    // no data yet → `refreshing` (no fabricated quota); resolved → headline.
    let mut cache = UsageCache::default();

    // Before start: no focused agent → status bar renders nothing.
    assert_eq!(cache.focused_status_bar_label(None, None), None);

    // Started, not yet resolved → refreshing on both surfaces.
    assert_eq!(
        cache.focused_status_bar_label(Some("codex"), Some("OpenAI")),
        Some("refreshing".to_owned())
    );
    let refreshing = cache.focused_snapshot(Some("codex"), Some("OpenAI"));
    assert_eq!(refreshing.status_bar_label, "refreshing");
    assert!(
        refreshing.buckets.is_empty(),
        "refreshing must carry no fabricated quota"
    );

    // Resolved: a cached snapshot wins and the real headline renders.
    cache.snapshots.insert(
        canonical_usage_cache_key("codex", Some("OpenAI")),
        CachedUsage {
            view: codex_cached_usage_view(),
        },
    );
    let resolved = cache.focused_snapshot(Some("codex"), Some("OpenAI"));
    assert_ne!(resolved.status_bar_label, "refreshing");
    assert!(!resolved.buckets.is_empty());
}

#[test]
fn account_snapshot_rows_carry_reset_epoch() {
    // the CLI report (`usage accounts`) emits the raw reset epoch, not
    // a dropped null — so the CLI and TUI agree on reset data.
    let now = 1_782_000_000;
    let reset_at = now + 3_600;
    let mut view = codex_cached_usage_view();
    view.buckets = vec![timed_bucket(
        "Session",
        Some("7% used".to_owned()),
        Some("100%".to_owned()),
        Some(93),
        Some(reset_at),
        now,
        None,
        UsageSnapshotStatus::Fresh,
    )];
    let mut snapshots = HashMap::new();
    snapshots.insert("codex".to_owned(), CachedUsage { view });
    let rows = account_snapshot_views_from_cache(&snapshots);
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row");
    assert_eq!(session.resets_at, Some(reset_at));
}

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
