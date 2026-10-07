// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn two_claude_accounts_and_codex_produce_three_tabs_with_distinct_ids() {
    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100),
    );
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "b@example.com", Some("Max 20x"), 200),
    );
    cache.insert_snapshot_for_test(
        "codex",
        Some("OpenAI"),
        account_snapshot_view("OpenAI", "codex@example.com", Some("Pro 20x"), 150),
    );

    let snapshot = cache.focused_snapshot(Some("claude"), Some("Anthropic"));

    // One tab (and therefore one overview row) per admitted account.
    assert_eq!(snapshot.tabs.len(), 3);
    let mut ids: Vec<String> = snapshot.tabs.iter().map(|tab| tab.id.clone()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3);
    let mut expected = vec![
        usage_account_tab_id("Anthropic", "a@example.com"),
        usage_account_tab_id("Anthropic", "b@example.com"),
        usage_account_tab_id("OpenAI", "codex@example.com"),
    ];
    expected.sort();
    assert_eq!(ids, expected);
    // The focused account (newest Claude fetch) is the active tab.
    let active: Vec<&UsageProviderTab> = snapshot.tabs.iter().filter(|tab| tab.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(
        active[0].id,
        usage_account_tab_id("Anthropic", "b@example.com")
    );

    // Selection by id focuses the correct account: a view focused on the
    // other Claude account marks exactly its tab, matched by id rather than
    // the shared "Anthropic" display label.
    let id_a = usage_account_tab_id("Anthropic", "a@example.com");
    let mut selected = account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100);
    enrich_provider_tabs(&mut selected, &cache.snapshots);
    mark_active_tab(&mut selected);
    let active: Vec<&UsageProviderTab> = selected.tabs.iter().filter(|tab| tab.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id_a);
    assert_eq!(active[0].account_label, "a@example.com");
    // Strip labels stay individually visible per account.
    let mut labels: Vec<String> = selected.tabs.iter().map(|tab| tab.label.clone()).collect();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), 3);
}

#[test]
fn focused_snapshot_for_account_id_selects_exact_account() {
    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "a@example.com", Some("Max"), 100),
    );
    cache.insert_snapshot_for_test(
        "claude",
        Some("Anthropic"),
        account_snapshot_view("Anthropic", "b@example.com", Some("Max 20x"), 200),
    );
    let id_b = usage_account_tab_id("Anthropic", "b@example.com");

    let snapshot = cache
        .focused_snapshot_for_account_id(&id_b)
        .expect("snapshot for claude-b");
    assert_eq!(snapshot.account.account_label, "b@example.com");
    assert_eq!(snapshot.tabs.len(), 2);
    let active: Vec<&UsageProviderTab> = snapshot.tabs.iter().filter(|tab| tab.active).collect();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id_b);

    assert!(
        cache
            .focused_snapshot_for_account_id("sha256:unknown")
            .is_none()
    );
    assert!(cache.focused_snapshot_for_account_id("").is_none());
}

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
fn first_credential_uses_home_first_then_handoff_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home.credentials.json");
    let handoff = dir.path().join("handoff.credentials.json");
    // Home present but WITHOUT a usable token — the proven in-container
    // failure mode — so resolution must fall through to the forwarded
    // handoff rather than dropping to the impoverished CLI path.
    fs::write(&home, r#"{"oauthAccount":{"emailAddress":"a@b.c"}}"#).expect("write home");
    fs::write(
        &handoff,
        r#"{"claudeAiOauth":{"accessToken":"handoff-token"}}"#,
    )
    .expect("write handoff");
    let resolved = first_credential(
        &[home.clone(), handoff.clone()],
        load_claude_oauth_credentials,
    );
    assert_eq!(
        resolved.map(|c| c.access_token),
        Some("handoff-token".to_owned())
    );
    // A valid home token wins over the handoff (home is the source of truth).
    fs::write(&home, r#"{"claudeAiOauth":{"accessToken":"home-token"}}"#).expect("rewrite home");
    let resolved = first_credential(&[home, handoff], load_claude_oauth_credentials);
    assert_eq!(
        resolved.map(|c| c.access_token),
        Some("home-token".to_owned())
    );
}

#[test]
fn codex_rpc_maps_spark_windows_and_reset_credits() {
    // Mirrors the live `account/rateLimits/read` response: the main "codex"
    // limit is Session/Weekly; a separate "…Codex-Spark" entry under
    // rateLimitsByLimitId carries the Spark windows; rateLimitResetCredits
    // carries the manual-reset count.
    let body = r#"{
            "rateLimits": {"limitId": "codex",
                "primary": {"usedPercent": 7, "windowDurationMins": 300, "resetsAt": 1782396144},
                "secondary": {"usedPercent": 5, "windowDurationMins": 10080, "resetsAt": 1782940724},
                "credits": {"hasCredits": false, "unlimited": false, "balance": "0"},
                "planType": "pro"},
            "rateLimitsByLimitId": {
                "codex_bengalfox": {"limitId": "codex_bengalfox", "limitName": "GPT-5.3-Codex-Spark",
                    "primary": {"usedPercent": 0, "windowDurationMins": 300, "resetsAt": 1782411283},
                    "secondary": {"usedPercent": 0, "windowDurationMins": 10080, "resetsAt": 1782998083}},
                "codex": {"limitId": "codex",
                    "primary": {"usedPercent": 7, "windowDurationMins": 300, "resetsAt": 1782396144},
                    "secondary": {"usedPercent": 5, "windowDurationMins": 10080, "resetsAt": 1782940724}}
            },
            "rateLimitResetCredits": {"availableCount": 2}
        }"#;
    let limits: CodexRpcRateLimitsResponse =
        serde_json::from_str(body).expect("decode rateLimits response");
    let usage = CodexRpcUsage::from_rpc(limits, None);
    let labels: Vec<String> = usage
        .response
        .buckets(1_782_300_000)
        .into_iter()
        .map(|b| b.label)
        .collect();
    assert!(labels.contains(&"Session".to_owned()));
    assert!(labels.contains(&"Weekly".to_owned()));
    assert!(labels.contains(&"Codex Spark 5-hour".to_owned()));
    assert!(labels.contains(&"Codex Spark Weekly".to_owned()));
    assert!(labels.contains(&"Limit Reset Credits".to_owned()));
    // The main "codex" limit must not be duplicated as an extra limit.
    assert_eq!(labels.iter().filter(|l| l.as_str() == "Session").count(), 1);
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
