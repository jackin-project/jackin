use super::*;

#[test]
fn compact_count_uses_token_suffixes() {
    assert_eq!(compact_count(999), "999");
    assert_eq!(compact_count(1_500), "1.5K");
    assert_eq!(compact_count(2_000_000), "2.0M");
}

#[test]
fn provider_labels_resolve_all_account_refresh_surfaces() {
    assert_eq!(
        resolve_surface("codex", Some("Claude")),
        UsageSurface::Claude
    );
    assert_eq!(
        resolve_surface("claude", Some("Codex")),
        UsageSurface::Codex
    );
    assert_eq!(resolve_surface("codex", Some("Amp")), UsageSurface::Amp);
    assert_eq!(
        resolve_surface("claude", Some("Grok Build")),
        UsageSurface::Grok
    );
    assert_eq!(
        resolve_surface("codex", Some("GLM / Z.AI")),
        UsageSurface::Zai
    );
    assert_eq!(resolve_surface("codex", Some("Kimi")), UsageSurface::Kimi);
    assert_eq!(
        resolve_surface("codex", Some("MiniMax")),
        UsageSurface::Minimax
    );
}

#[test]
fn provider_tabs_follow_usage_overlay_display_order() {
    let labels = provider_tabs(UsageSurface::Codex)
        .into_iter()
        .map(|tab| tab.label)
        .collect::<Vec<_>>();

    assert_eq!(
        labels,
        vec![
            "Codex".to_owned(),
            "Claude".to_owned(),
            "Amp".to_owned(),
            "Grok Build".to_owned(),
            "GLM / Z.AI".to_owned(),
            "Kimi".to_owned(),
            "MiniMax".to_owned(),
        ]
    );
}

#[test]
fn provider_tabs_include_cached_account_identity() {
    let mut view = FocusedUsageView::unavailable("none", 123);
    view.account = FocusedAccountHeader {
        provider_label: "OpenAI / Codex".to_owned(),
        account_label: "codex@example.com".to_owned(),
        username: None,
        plan_label: Some("Pro 20x".to_owned()),
        credential_origin: None,
    };
    view.status = UsageSnapshotStatus::Fresh;
    view.tabs = provider_tabs(UsageSurface::Codex);

    let mut claude = FocusedUsageView::unavailable("none", 120);
    claude.account = FocusedAccountHeader {
        provider_label: "Anthropic / Claude".to_owned(),
        account_label: "claude@example.com".to_owned(),
        username: None,
        plan_label: Some("Max".to_owned()),
        credential_origin: None,
    };
    claude.status = UsageSnapshotStatus::Stale;

    let mut snapshots = HashMap::new();
    snapshots.insert("claude:Claude".to_owned(), CachedUsage { view: claude });

    enrich_provider_tabs(&mut view, &snapshots);

    let codex = view
        .tabs
        .iter()
        .find(|tab| tab.label == "Codex")
        .expect("codex tab");
    assert_eq!(codex.account_label, "codex@example.com");
    assert_eq!(codex.plan_label.as_deref(), Some("Pro 20x"));

    let claude = view
        .tabs
        .iter()
        .find(|tab| tab.label == "Claude")
        .expect("claude tab");
    assert_eq!(claude.account_label, "claude@example.com");
    assert_eq!(claude.plan_label.as_deref(), Some("Max"));
    assert_eq!(claude.status_label, "stale");
}

#[test]
fn claude_account_email_reads_oauth_account_metadata() {
    // the email identity comes from `oauthAccount.emailAddress`.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        r#"{"oauthAccount":{"emailAddress":"alexey@example.com"}}"#,
    )
    .expect("write");
    assert_eq!(
        load_claude_account_email(&path).as_deref(),
        Some("alexey@example.com")
    );

    let empty = dir.path().join("empty.json");
    fs::write(&empty, r#"{"oauthAccount":{}}"#).expect("write");
    assert_eq!(load_claude_account_email(&empty), None);

    let none = dir.path().join("none.json");
    fs::write(&none, "{}").expect("write");
    assert_eq!(load_claude_account_email(&none), None);
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
fn claude_oauth_usage_decodes_live_api_body() {
    // Mirrors the live api.anthropic.com/api/oauth/usage 200 body: `seven_day`
    // and `seven_day_oauth_apps` are SEPARATE keys (they must not collide on
    // one field), plus new codename windows the model must tolerate.
    let body = r#"{
            "five_hour": {"utilization": 12, "resets_at": "2026-06-25T19:00:00Z"},
            "seven_day": {"utilization": 34, "resets_at": "2026-06-26T14:00:00Z"},
            "seven_day_oauth_apps": null,
            "seven_day_sonnet": {"utilization": 5, "resets_at": "2026-06-26T14:00:00Z"},
            "seven_day_opus": null,
            "seven_day_cowork": null,
            "seven_day_omelette": null,
            "amber_ladder": null, "cinder_cove": null, "iguana_necktie": null,
            "omelette_promotional": null, "tangelo": null,
            "extra_usage": {"is_enabled": false, "monthly_limit": 0, "used_credits": 0,
                "utilization": 0, "currency": "USD", "decimal_places": 2,
                "disabled_reason": "x", "daily": null, "weekly": null},
            "limits": [{"kind": "x", "group": "x", "percent": 0, "severity": "x",
                "resets_at": "x", "scope": null, "is_active": false}],
            "spend": null
        }"#;
    let parsed: ClaudeOAuthUsageResponse =
        serde_json::from_str(body).expect("decode live OAuth usage body");
    assert!(parsed.five_hour.is_some());
    assert!(parsed.seven_day.is_some());
    assert!(parsed.seven_day_sonnet.is_some());
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
fn usage_status_label_prefers_in_memory_cache_before_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cache = UsageCache::default();
    cache.set_telemetry_store_path(dir.path().join("missing").join("usage.sqlite3"));
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
fn usage_snapshot_prefers_in_memory_cache_before_store() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut cache = UsageCache::default();
    cache.set_telemetry_store_path(dir.path().join("missing").join("usage.sqlite3"));
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
            .any(|tab| tab.label == "Codex" && tab.active)
    );
}

#[test]
fn usage_status_label_does_not_read_store_on_cache_miss() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.sqlite3");
    crate::telemetry_store::store_usage_snapshot(&db, &codex_cached_usage_view())
        .expect("store usage snapshot");
    let mut cache = UsageCache::default();
    cache.set_telemetry_store_path(db);

    // A focused agent with no cached snapshot is mid-load → `refreshing`
    // (P3), computed without touching the store.
    assert_eq!(
        cache.focused_status_bar_label(Some("codex"), Some("OpenAI")),
        Some("refreshing".to_owned())
    );
}

#[test]
fn usage_snapshot_does_not_read_store_on_cache_miss() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.sqlite3");
    crate::telemetry_store::store_usage_snapshot(&db, &codex_cached_usage_view())
        .expect("store usage snapshot");
    let mut cache = UsageCache::default();
    cache.set_telemetry_store_path(db);

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
    assert_eq!(accounts[0].provider, "OpenAI / Codex");
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

fn codex_cached_usage_view() -> FocusedUsageView {
    usage_view(UsageViewInput {
        agent: "codex",
        provider: Some("OpenAI"),
        surface: UsageSurface::Codex,
        account_label: "codex@example.com".to_owned(),
        username: None,
        plan_label: Some("Pro 20x".to_owned()),
        credential_origin: None,
        buckets: vec![QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Session".to_owned(),
            used_label: Some("63% used".to_owned()),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(37),
            reset_label: Some("Resets in 2h".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        now: 123,
        last_error: None,
    })
}

#[test]
fn materialized_usage_accounts_write_normalized_snapshots() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("usage").join("accounts.json");
    let mut view = FocusedUsageView::unavailable("none", 123);
    view.focused_agent = Some("codex".to_owned());
    view.status_bar_label = "Codex Session: 63% used · 37% left".to_owned();

    write_materialized_usage_accounts(&path, 456, vec![view]).expect("write accounts");

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
fn status_bar_label_uses_amp_free_and_credits() {
    let buckets = vec![
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Amp Free".to_owned(),
            used_label: Some("$5.24".to_owned()),
            limit_label: Some("$10".to_owned()),
            remaining_percent: Some(48),
            reset_label: None,
            resets_at: None,
            status_slot: None,
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

    assert_eq!(
        status_bar_label(
            UsageSurface::Amp,
            "alexey@example.com",
            UsageSnapshotStatus::Fresh,
            &buckets
        ),
        "Free 48% · $4.76"
    );
}

#[test]
fn status_bar_label_uses_stale_amp_cache() {
    let buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Amp Free".to_owned(),
        used_label: Some("$9.10".to_owned()),
        limit_label: Some("$10".to_owned()),
        remaining_percent: Some(9),
        reset_label: None,
        resets_at: None,
        status_slot: None,
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
fn usage_refresh_targets_are_focused_first_and_deduplicated() {
    let active = vec![
        UsageRefreshTarget {
            agent: "claude".to_owned(),
            provider: Some("Anthropic".to_owned()),
        },
        UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
        },
        UsageRefreshTarget {
            agent: "claude".to_owned(),
            provider: Some("Anthropic / Claude".to_owned()),
        },
    ];
    let focused = UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("OpenAI".to_owned()),
    };

    let ordered = ordered_refresh_targets(&active, Some(focused));

    assert_eq!(
        ordered,
        vec![
            UsageRefreshTarget {
                agent: "codex".to_owned(),
                provider: Some("OpenAI".to_owned()),
            },
            UsageRefreshTarget {
                agent: "claude".to_owned(),
                provider: Some("Anthropic".to_owned()),
            },
        ]
    );
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "test worker sleeps on owned scoped threads to prove overlapping probes"
)]
fn usage_refresh_probes_are_spawned_before_any_join() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    let targets = vec![
        UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
        },
        UsageRefreshTarget {
            agent: "claude".to_owned(),
            provider: Some("Anthropic".to_owned()),
        },
    ];
    let active = Arc::new(AtomicUsize::new(0));
    let max_active = Arc::new(AtomicUsize::new(0));

    let results = collect_usage_refresh_results(targets, {
        let active = Arc::clone(&active);
        let max_active = Arc::clone(&max_active);
        move |target| {
            let now_active = active.fetch_add(1, AtomicOrdering::SeqCst) + 1;
            max_active.fetch_max(now_active, AtomicOrdering::SeqCst);
            thread::sleep(Duration::from_millis(75));
            active.fetch_sub(1, AtomicOrdering::SeqCst);
            UsageRefreshResult {
                target,
                view: FocusedUsageView::unavailable("test", now_epoch()),
                codex_rpc_gate: ManagedCliLaunchGate::default(),
                grok_rpc_gate: ManagedCliLaunchGate::default(),
            }
        }
    });

    assert_eq!(results.len(), 2);
    assert!(
        max_active.load(AtomicOrdering::SeqCst) >= 2,
        "refresh probes were joined serially instead of overlapping"
    );
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "test worker sleeps on an owned thread to prove timeout fallback"
)]
fn usage_refresh_probe_timeout_returns_fallback_result() {
    let targets = vec![
        UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
        },
        UsageRefreshTarget {
            agent: "claude".to_owned(),
            provider: Some("Anthropic".to_owned()),
        },
    ];

    let results = collect_usage_refresh_results_with_timeout(
        targets,
        |target| {
            if target.provider.as_deref() == Some("Anthropic") {
                thread::sleep(Duration::from_millis(250));
            }
            UsageRefreshResult {
                target,
                view: FocusedUsageView::unavailable("test", now_epoch()),
                codex_rpc_gate: ManagedCliLaunchGate::default(),
                grok_rpc_gate: ManagedCliLaunchGate::default(),
            }
        },
        Duration::from_millis(25),
    );

    assert_eq!(results.len(), 2);
    let timed_out = results
        .iter()
        .find(|result| result.target.provider.as_deref() == Some("Anthropic"))
        .expect("timed-out provider fallback");
    assert_eq!(
        timed_out.view.last_error.as_deref(),
        Some("usage provider probe timed out")
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
    assert_ne!(
        canonical_usage_cache_key("claude", Some("Anthropic")),
        canonical_usage_cache_key("claude", Some("Z.AI"))
    );
}

#[test]
fn usage_refresh_interval_stays_within_jitter_bounds() {
    for key in ["Codex", "Claude", "GLM / Z.AI"] {
        let interval = refresh_interval_for_key(key);
        assert!(
            interval >= USAGE_REFRESH_BASE_INTERVAL.saturating_sub(USAGE_REFRESH_JITTER),
            "{key}: {interval:?}"
        );
        assert!(
            interval <= USAGE_REFRESH_BASE_INTERVAL + USAGE_REFRESH_JITTER,
            "{key}: {interval:?}"
        );
    }
}

#[test]
fn usage_rate_limit_delay_honors_retry_after_and_caps_backoff() {
    assert_eq!(
        usage_rate_limit_delay("provider HTTP 429 retry-after: 17", 1),
        Duration::from_secs(17)
    );
    assert_eq!(
        usage_rate_limit_delay("provider HTTP 429", 1),
        USAGE_REFRESH_BASE_INTERVAL
    );
    assert_eq!(
        usage_rate_limit_delay("provider HTTP 429", 20),
        USAGE_REFRESH_BACKOFF_CAP
    );
    assert!(!usage_error_is_rate_limited("provider HTTP 500"));
}

#[test]
fn usage_refresh_schedule_skips_until_ttl_or_manual_refresh() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("OpenAI".to_owned()),
    };
    let mut schedule = UsageRefreshSchedule::default();
    let now = Instant::now();
    let view = FocusedUsageView::unavailable("fresh", now_epoch());

    assert!(schedule.should_refresh_with_cooldown_dir(&target, now, dir.path()));
    schedule.mark_refreshed_with_cooldown_dir(&target, now, &view, dir.path(), dir.path());
    assert!(!schedule.should_refresh_with_cooldown_dir(
        &target,
        now + Duration::from_secs(30),
        dir.path()
    ));

    schedule.mark_due(&target, now + Duration::from_secs(31));
    assert!(schedule.should_refresh_with_cooldown_dir(
        &target,
        now + Duration::from_secs(31),
        dir.path()
    ));
}

#[test]
fn usage_refresh_schedule_writes_and_honors_shared_rate_limit_cooldown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("OpenAI".to_owned()),
    };
    let mut schedule = UsageRefreshSchedule::default();
    let now = Instant::now();

    assert!(schedule.should_refresh_with_cooldown_dir(&target, now, dir.path()));

    let mut view = FocusedUsageView::unavailable("rate limited", now_epoch());
    view.last_error = Some("Codex usage HTTP 429 retry-after: 60".to_owned());
    schedule.mark_refreshed_with_cooldown_dir(&target, now, &view, dir.path(), dir.path());

    let key = target.cache_key();
    assert!(shared_usage_cooldown_active(dir.path(), &key, now_epoch()));
    assert!(!schedule.should_refresh_with_cooldown_dir(
        &target,
        now + Duration::from_secs(61),
        dir.path()
    ));
}

#[test]
fn successful_refresh_writes_shared_cooldown_and_snapshot() {
    let cooldown_dir = tempfile::tempdir().expect("tempdir");
    let snapshots_dir = tempfile::tempdir().expect("tempdir");
    let target = UsageRefreshTarget {
        agent: "claude".to_owned(),
        provider: None,
    };
    let mut schedule = UsageRefreshSchedule::default();
    let now = Instant::now();
    let view = FocusedUsageView::unavailable("fresh", now_epoch());

    assert!(schedule.should_refresh_with_cooldown_dir(&target, now, cooldown_dir.path()));

    schedule.mark_refreshed_with_cooldown_dir(
        &target,
        now,
        &view,
        cooldown_dir.path(),
        snapshots_dir.path(),
    );

    let key = target.cache_key();
    // Shared cooldown marker written for success.
    assert!(
        shared_usage_cooldown_active(cooldown_dir.path(), &key, now_epoch()),
        "success cooldown marker should be active after successful refresh"
    );
    // Fresh instance (no in-process state) sees cooldown → skips fetch.
    let mut fresh_schedule = UsageRefreshSchedule::default();
    assert!(
        !fresh_schedule.should_refresh_with_cooldown_dir(&target, now, cooldown_dir.path()),
        "fresh instance should skip fetch when success cooldown is active"
    );
    // Shared snapshot written and readable.
    assert!(
        read_shared_usage_snapshot(snapshots_dir.path(), &key).is_some(),
        "shared snapshot should be readable after successful refresh"
    );
}

#[test]
fn fresh_instance_seeds_cache_from_shared_snapshot_when_cooldown_active() {
    let snapshots_dir = tempfile::tempdir().expect("tempdir");
    let key = "Claude Max";
    let view = FocusedUsageView::unavailable("seed", now_epoch());
    // Write a snapshot as if another instance had already refreshed.
    write_shared_usage_snapshot(snapshots_dir.path(), key, &view);
    // Another instance reads it back.
    let loaded = read_shared_usage_snapshot(snapshots_dir.path(), key);
    assert!(
        loaded.is_some(),
        "shared snapshot should be readable for seeding a fresh instance"
    );
}

#[test]
fn failed_refresh_preserves_last_fresh_quota_rows_as_stale_cache() {
    let mut cached = FocusedUsageView::unavailable("seed", 123);
    cached.status = UsageSnapshotStatus::Fresh;
    cached.confidence = UsageConfidence::Authoritative;
    cached.account = FocusedAccountHeader {
        provider_label: "OpenAI / Codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        username: None,
        plan_label: Some("Pro 20x".to_owned()),
        credential_origin: None,
    };
    cached.buckets = vec![QuotaBucketView {
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
    }];

    for failed_status in [
        UsageSnapshotStatus::Stale,
        UsageSnapshotStatus::NeedsLogin,
        UsageSnapshotStatus::Error,
    ] {
        let mut view = FocusedUsageView::unavailable("seed", 124);
        view.focused_agent = Some("codex".to_owned());
        view.focused_provider = Some("Codex".to_owned());
        view.status = failed_status;
        view.account = FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: None,
            credential_origin: None,
        };
        view.last_error = Some("Codex provider usage unavailable".to_owned());

        preserve_cached_quota_on_failed_refresh(&mut view, &cached);

        assert_eq!(view.status, UsageSnapshotStatus::Stale);
        assert_eq!(view.source, UsageSource::Cache);
        assert_eq!(view.confidence, UsageConfidence::Authoritative);
        assert_eq!(view.buckets.len(), 1);
        assert_eq!(view.buckets[0].status, UsageSnapshotStatus::Stale);
        assert_eq!(view.account.plan_label.as_deref(), Some("Pro 20x"));
        assert_eq!(view.status_bar_label, "Weekly 10%");
        assert!(
            view.last_error
                .as_deref()
                .is_some_and(|error| error.contains("showing last cached quota"))
        );
    }
}

#[test]
fn claude_oauth_response_maps_windows_to_buckets() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 0.84, "resets_at": "2026-06-11T15:12:00Z" },
        "seven_day": { "utilization": 0.78, "resets_at": "2026-06-12T14:26:00Z" },
        "seven_day_sonnet": { "utilization": 0.02, "resets_at": "2026-06-12T14:26:00Z" },
        "seven_day_routines": { "utilization": 0.0 },
        // Real API shape: credits are MINOR units (cents) with `decimal_places`,
        // and `utilization` is a percent (0..100). No `spend` object here, so this
        // exercises the `extra_usage` fallback path.
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 26000.0,
            "used_credits": 7849.0,
            "utilization": 30.0,
            "currency": "SGD",
            "decimal_places": 2
        }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);

    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].remaining_percent, Some(16));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        Some(
            reset_label(
                parse_iso_epoch("2026-06-11T15:12:00Z").expect("session reset"),
                1_781_185_560,
            )
            .as_str()
        )
    );
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, Some(22));
    // Sonnet / Daily Routines fill no headline slot.
    assert!(buckets.iter().any(|bucket| bucket.label == "Sonnet"));
    assert!(
        buckets
            .iter()
            .find(|bucket| bucket.label == "Sonnet")
            .is_some_and(|bucket| bucket.status_slot.is_none())
    );
    assert!(
        buckets
            .iter()
            .find(|bucket| bucket.label == "Sonnet")
            .is_some_and(|bucket| bucket.pace_label.is_none())
    );
    assert!(buckets.iter().any(|bucket| bucket.label == "Daily Routines"
        && bucket.remaining_percent == Some(100)
        && bucket.pace_label.is_none()
        && bucket.status_slot.is_none()));
    // `seven_day_opus` was absent from the response — it must be omitted
    // entirely, never fabricated into a (full-meter) row.
    assert!(!buckets.iter().any(|bucket| bucket.label == "Opus"));
    let extra = buckets
        .iter()
        .find(|bucket| bucket.label == "Extra usage")
        .expect("extra usage bucket");
    // spent vs cap — `<currency> <spent> spent` + `NN% used`. Minor units are
    // scaled by `decimal_places` (7849 → 78.49), and the bucket fills the Spend
    // slot carrying structured Money for the status-bar chunk.
    assert_eq!(extra.status_slot, Some(StatusSlot::Spend));
    assert_eq!(extra.remaining_percent, Some(70));
    assert_eq!(extra.used_label.as_deref(), Some("SGD 78.49 spent"));
    assert_eq!(extra.limit_label.as_deref(), Some("SGD 260.00"));
    assert_eq!(extra.pace_label.as_deref(), Some("30% used"));
    assert_eq!(
        extra.used_money.as_ref().map(Money::to_string).as_deref(),
        Some("SGD 78.49")
    );
    assert_eq!(
        extra
            .used_money
            .as_ref()
            .map(Money::format_compact)
            .as_deref(),
        Some("SGD 78")
    );
}

/// The self-describing `spend{}` object is preferred over `extra_usage` and
/// reproduces the web console's Enterprise figure exactly ($53.31 / $300.00,
/// 18% used) — the regression guard for the 100×-too-large bug.
#[test]
fn claude_spend_object_preferred_and_scaled() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        // Enterprise responses carry no rolling windows, only spend.
        "five_hour": null,
        "seven_day": null,
        // A stale/raw extra_usage is also present; spend{} must win.
        "extra_usage": {
            "is_enabled": true,
            "monthly_limit": 30000.0,
            "used_credits": 5331.0,
            "utilization": 17.77,
            "currency": "USD",
            "decimal_places": 2
        },
        "spend": {
            "used": { "amount_minor": 5331, "currency": "USD", "exponent": 2 },
            "limit": { "amount_minor": 30000, "currency": "USD", "exponent": 2 },
            "percent": 18,
            "severity": "normal",
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let spend = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("spend bucket");
    assert_eq!(spend.used_label.as_deref(), Some("$53.31 spent"));
    assert_eq!(spend.limit_label.as_deref(), Some("$300.00"));
    assert_eq!(spend.pace_label.as_deref(), Some("18% used"));
    assert_eq!(spend.remaining_percent, Some(82));
    assert_eq!(spend.severity, UsageSeverity::Normal);

    // The headline renders compact money as `<used> of <limit>`, currency once.
    assert_eq!(
        spend_headline_label(&buckets).as_deref(),
        Some("$53 of 300")
    );
}

#[test]
fn codex_refresh_request_body_uses_refresh_grant() {
    let body = codex_refresh_request_body("rt-abc");
    assert_eq!(body["grant_type"], "refresh_token");
    assert_eq!(body["refresh_token"], "rt-abc");
    assert_eq!(body["client_id"], CODEX_OAUTH_CLIENT_ID);
    assert!(
        !CODEX_OAUTH_CLIENT_ID.is_empty(),
        "client id must be set for the refresh grant"
    );
}

#[test]
fn codex_access_token_parsed_from_refresh_response() {
    let value = serde_json::json!({ "access_token": "  new-token  ", "token_type": "Bearer" });
    assert_eq!(
        codex_access_token_from_response(&value).as_deref(),
        Some("new-token")
    );
    // Missing / empty token yields None so the caller falls back to NeedsLogin.
    assert!(codex_access_token_from_response(&serde_json::json!({})).is_none());
    assert!(codex_access_token_from_response(&serde_json::json!({ "access_token": "" })).is_none());
}

#[test]
fn codex_oauth_credentials_carry_refresh_token() {
    let value = serde_json::json!({
        "tokens": {
            "access_token": "at-1",
            "refresh_token": "rt-1",
            "account_id": "acct-1"
        }
    });
    let creds = codex_oauth_from_value(&value).expect("codex credentials");
    assert_eq!(creds.access_token, "at-1");
    assert_eq!(creds.refresh_token.as_deref(), Some("rt-1"));
    // A static API key has nothing to refresh.
    let api = codex_oauth_from_value(&serde_json::json!({ "OPENAI_API_KEY": "sk-x" }))
        .expect("api key credentials");
    assert!(api.refresh_token.is_none());
}

#[test]
fn unauthorized_errors_are_distinguished_from_transient() {
    assert!(usage_error_is_unauthorized(
        "Codex OAuth usage HTTP 401 Unauthorized"
    ));
    assert!(usage_error_is_unauthorized(
        "Claude OAuth usage HTTP 403 Forbidden"
    ));
    assert!(!usage_error_is_unauthorized("Codex OAuth usage HTTP 500"));
    assert!(!usage_error_is_unauthorized("request failed: timed out"));
    // A rate-limit is transient, not an auth failure.
    assert!(!usage_error_is_unauthorized("usage HTTP 429 rate limit"));
}

/// Rotating-codename dollar-budget windows (enterprise contractual
/// allocations) surface as dollar buckets instead of being dropped by the
/// fixed-field struct; zero/absent ones are ignored.
#[test]
fn claude_codename_dollar_window_is_surfaced() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "amber_ladder": {
            "utilization": 0.0,
            "resets_at": "2026-09-02T06:59:59+00:00",
            "limit_dollars": 25000,
            "used_dollars": 5000.0
        },
        // Present but empty — must not produce a bucket.
        "omelette_promotional": { "utilization": 0.0, "limit_dollars": null }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let amber = buckets
        .iter()
        .find(|bucket| bucket.label == "Amber Ladder")
        .expect("amber_ladder dollar window surfaced");
    assert_eq!(amber.used_label.as_deref(), Some("$5000.00 spent"));
    assert_eq!(amber.limit_label.as_deref(), Some("$25000.00"));
    assert_eq!(amber.remaining_percent, Some(80));
    assert_eq!(amber.status_slot, None);
    assert!(
        !buckets
            .iter()
            .any(|bucket| bucket.label.contains("omelette")),
        "a null-budget codename window must not produce a bucket"
    );
}

/// A disabled (out-of-credits) spend window is still surfaced — with its
/// reason — instead of being silently dropped, so the cap stays visible.
#[test]
fn claude_spend_disabled_is_surfaced_with_reason() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "spend": {
            "used": { "amount_minor": 7849, "currency": "SGD", "exponent": 2 },
            "limit": { "amount_minor": 26000, "currency": "SGD", "exponent": 2 },
            "percent": 30,
            "severity": "normal",
            "enabled": false,
            "disabled_reason": "out_of_credits"
        }
    }))
    .expect("valid Claude OAuth usage");

    let buckets = usage.into_buckets(1_781_185_560);
    let spend = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Spend))
        .expect("disabled spend bucket is still present");
    assert_eq!(spend.used_label.as_deref(), Some("SGD 78.49 spent"));
    assert_eq!(
        spend.pace_label.as_deref(),
        Some("disabled · out of credits")
    );
    // Headline still shows the cap context: `<used> of <limit>`.
    assert_eq!(
        spend_headline_label(&buckets).as_deref(),
        Some("SGD 78 of 260")
    );
}

/// The status-bar headline joins the percentage windows and the monetary spend
/// into one ` · `-separated string.
#[test]
fn status_bar_headline_joins_windows_and_spend() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 11.0, "resets_at": "2026-06-28T16:40:00Z" },
        "seven_day": { "utilization": 27.0, "resets_at": "2026-07-03T07:00:00Z" },
        "spend": {
            "used": { "amount_minor": 7849, "currency": "SGD", "exponent": 2 },
            "limit": { "amount_minor": 26000, "currency": "SGD", "exponent": 2 },
            "percent": 30,
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");
    let buckets = usage.into_buckets(1_781_185_560);
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Claude, &buckets).as_deref(),
        Some("Session 89% · Weekly 73% · SGD 78 of 260")
    );
}

#[test]
fn fraction_helpers_reject_absent_and_clamp_present() {
    // Fraction form (0..=1) and already-percent form (>1) both map to a
    // clamped used percentage.
    assert_eq!(used_percent_from_fraction(0.0), Some(0));
    assert_eq!(used_percent_from_fraction(1.0), Some(100));
    assert_eq!(used_percent_from_fraction(0.84), Some(84));
    assert_eq!(used_percent_from_fraction(42.0), Some(42));
    assert_eq!(used_percent_from_fraction(150.0), Some(100));

    // Absent/unknown sentinels must yield None, never a fabricated value.
    // A negative input previously rendered as `Some(100)` — a "100% left"
    // row for data that is genuinely absent.
    assert_eq!(used_percent_from_fraction(-0.5), None);
    assert_eq!(used_percent_from_fraction(f64::NAN), None);
    assert_eq!(used_percent_from_fraction(f64::INFINITY), None);
    assert_eq!(used_percent_from_fraction(f64::NEG_INFINITY), None);

    // remaining = 100 - used, propagating None for absent data.
    assert_eq!(remaining_from_fraction(0.84), Some(16));
    assert_eq!(remaining_from_fraction(1.0), Some(0));
    assert_eq!(remaining_from_fraction(-0.5), None);
    assert_eq!(remaining_from_fraction(f64::NAN), None);

    // The "used" label tracks the same absence contract.
    assert_eq!(used_percent_label(0.84).as_deref(), Some("84% used"));
    assert_eq!(used_percent_label(-0.5), None);
    assert_eq!(used_percent_label(f64::NAN), None);
}

#[test]
fn claude_oauth_response_accepts_window_aliases() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 0.10 },
        "seven_day": { "utilization": 0.45 },
        "seven_day_opus": { "utilization": 0.30 },
        // `seven_day_oauth_apps` is a SEPARATE window, not an alias of
        // `seven_day` — it must be ignored, never override Weekly.
        "seven_day_oauth_apps": { "utilization": 0.99 },
        "seven_day_cowork": { "utilization": 0.25 }
    }))
    .expect("valid Claude OAuth usage aliases");

    let buckets = usage.into_buckets(1_781_185_560);

    assert!(
        buckets
            .iter()
            .any(|bucket| bucket.label == "Weekly" && bucket.remaining_percent == Some(55))
    );
    assert!(
        buckets
            .iter()
            .any(|bucket| bucket.label == "Daily Routines" && bucket.remaining_percent == Some(75))
    );
    // A present Opus window is a detail row, never a headline slot.
    assert!(
        buckets
            .iter()
            .any(|bucket| bucket.label == "Opus" && bucket.status_slot.is_none())
    );
}

#[test]
fn codex_oauth_response_maps_primary_weekly_spark_and_credits() {
    let mut usage: CodexUsageResponse = serde_json::from_value(serde_json::json!({
        "plan_type": "pro",
        "rate_limit": {
            "primary_window": {
                "used_percent": 63,
                "reset_at": 1781189520,
                "limit_window_seconds": 18000
            },
            "secondary_window": {
                "used_percent": 90,
                "reset_at": 1781197200,
                "limit_window_seconds": 604800
            }
        },
        "additional_rate_limits": [{
            "limit_name": "gpt-5.3-codex-spark",
            "rate_limit": {
                "primary_window": {
                    "used_percent": 0,
                    "reset_at": 1781200800,
                    "limit_window_seconds": 18000
                },
                "secondary_window": {
                    "used_percent": 0,
                    "reset_at": 1781798400,
                    "limit_window_seconds": 604800
                }
            }
        }],
        "credits": {
            "has_credits": true,
            "unlimited": false,
            "balance": "12.5"
        }
    }))
    .expect("valid Codex usage");
    usage.reset_credits = Some(CodexResetCredits {
        available_count: 2,
        credits: vec![
            CodexResetCredit {
                status: Some("available".to_owned()),
                expires_at: Some("2026-06-10T00:00:00Z".to_owned()),
            },
            CodexResetCredit {
                status: Some("available".to_owned()),
                expires_at: Some("2026-06-18T00:00:00Z".to_owned()),
            },
            CodexResetCredit {
                status: Some("redeemed".to_owned()),
                expires_at: Some("2026-06-17T00:00:00Z".to_owned()),
            },
        ],
    });

    let buckets = usage.buckets(1_781_185_560);

    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].remaining_percent, Some(37));
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].remaining_percent, Some(10));
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert!(buckets.iter().any(
        |bucket| bucket.label == "Codex Spark 5-hour" && bucket.remaining_percent == Some(100)
    ));
    // The per-feature Spark detail rows are not headline slots.
    assert!(buckets.iter().all(|bucket| {
        !bucket.label.starts_with("Codex Spark") || bucket.status_slot.is_none()
    }));
    let reset_credits = buckets
        .iter()
        .position(|bucket| bucket.label == "Limit Reset Credits")
        .expect("reset credits bucket");
    let reset_credit_label = format!(
        "2 manual resets available · Next expires {}",
        expiry_label(
            parse_iso_epoch("2026-06-18T00:00:00Z").expect("expiry epoch"),
            1_781_185_560
        )
    );
    assert_eq!(
        buckets[reset_credits].pace_label.as_deref(),
        Some(reset_credit_label.as_str())
    );
    let credits = buckets
        .iter()
        .enumerate()
        .find(|(_, bucket)| bucket.label == "Credits")
        .expect("credits bucket");
    assert!(reset_credits < credits.0);
    assert_eq!(credits.1.limit_label.as_deref(), Some("12.50 credits"));
}

#[test]
fn codex_rpc_response_maps_account_windows_and_credits() {
    let limits: CodexRpcRateLimitsResponse = serde_json::from_value(serde_json::json!({
        "rateLimits": {
            "primary": {
                "usedPercent": 63.0,
                "windowDurationMins": 300,
                "resetsAt": 1781189520
            },
            "secondary": {
                "usedPercent": 90.0,
                "windowDurationMins": 10080,
                "resetsAt": 1781798400
            },
            "credits": {
                "hasCredits": true,
                "unlimited": false,
                "balance": "12.5"
            },
            "planType": "pro"
        }
    }))
    .expect("valid Codex RPC rate limits");
    let account: CodexRpcAccountResponse = serde_json::from_value(serde_json::json!({
        "account": {
            "type": "chatgpt",
            "email": "person@example.com",
            "planType": "pro"
        }
    }))
    .expect("valid Codex RPC account");

    let usage = CodexRpcUsage::from_rpc(limits, Some(account));
    let buckets = usage.response.buckets(1_781_185_560);

    assert_eq!(usage.account_label.as_deref(), Some("person@example.com"));
    assert_eq!(usage.response.plan_type.as_deref(), Some("pro"));
    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].remaining_percent, Some(37));
    assert_eq!(buckets[0].pace_label.as_deref(), Some("15% in reserve"));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, Some(10));
    assert_eq!(buckets[1].pace_label.as_deref(), Some("1 week window"));
    let credits = buckets
        .iter()
        .find(|bucket| bucket.label == "Credits")
        .expect("credits bucket");
    assert_eq!(credits.limit_label.as_deref(), Some("12.50 credits"));
}

#[test]
fn managed_cli_launch_gate_cools_down_after_launch_failure() {
    let mut gate = ManagedCliLaunchGate::default();
    assert!(gate.can_launch("probe", Instant::now()).is_ok());

    gate.record_launch_failure("blocked".to_owned());

    let error = gate
        .can_launch("probe", Instant::now())
        .expect_err("cooldown should block launch");
    assert!(error.contains("cooldown active"));
    assert!(error.contains("blocked"));

    gate.record_success();
    assert!(gate.can_launch("probe", Instant::now()).is_ok());
}

#[test]
fn claude_usage_diagnostic_invokes_explicit_usage_command() {
    let diagnostic = run_claude_usage_diagnostic_with(|command, args, timeout| {
        assert_eq!(command, "claude");
        assert_eq!(args, ["-p", "/usage"]);
        assert_eq!(timeout, PROVIDER_CLI_TIMEOUT);
        Ok(CliOutput {
            success: true,
            exit_code: Some(0),
            stdout: "usage output".to_owned(),
            stderr: String::new(),
        })
    })
    .expect("diagnostic");

    assert_eq!(diagnostic.command, "claude");
    assert_eq!(diagnostic.args, vec!["-p", "/usage"]);
    assert!(diagnostic.success);
    assert_eq!(diagnostic.stdout, "usage output");
}

#[test]
fn claude_usage_diagnostic_preserves_cli_failure_output() {
    let diagnostic = run_claude_usage_diagnostic_with(|_, _, _| {
        Ok(CliOutput {
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "not logged in".to_owned(),
        })
    })
    .expect("diagnostic");

    assert!(!diagnostic.success);
    assert_eq!(diagnostic.exit_code, Some(1));
    assert_eq!(diagnostic.stderr, "not logged in");
}

#[test]
fn claude_cli_usage_output_maps_current_windows() {
    let usage = parse_claude_usage_output(
        "You are currently using your subscription to power your Claude Code usage\n\
             \n\
             Current session: 0% used\n\
             Current week (all models): 46% used · resets Jun 26, 6:59am (UTC)\n\
             Current week (Sonnet only): 15% used · resets Jun 26, 6:59am (UTC)\n",
    )
    .expect("usage output");

    let buckets = usage.buckets();

    assert_eq!(buckets[0].label, "Session");
    assert_eq!(buckets[0].remaining_percent, Some(100));
    // The CLI fallback still fills the headline slots (regression guard:
    // OAuth-fetch failure must not blank the Claude status bar).
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].remaining_percent, Some(54));
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[2].label, "Sonnet");
    assert_eq!(buckets[2].remaining_percent, Some(85));
    assert_eq!(buckets[2].status_slot, None);

    // End-to-end: the tagged CLI buckets still render the Claude headline, so
    // an OAuth-fetch failure that drops to the CLI path does not blank it.
    assert_eq!(
        status_bar_label(
            UsageSurface::Claude,
            "",
            UsageSnapshotStatus::Fresh,
            &buckets
        ),
        "Session 100% · Weekly 54%"
    );
}

#[test]
fn provider_matches_usage_label_resolves_canonical_synonyms() {
    // A tab label matches an account provider label when both resolve to the
    // same canonical surface, across synonym spellings and in both orders.
    for (left, right) in [
        ("OpenAI / Codex", "codex"),
        ("Codex", "openai"),
        ("Anthropic / Claude", "claude"),
        ("Claude", "anthropic"),
        ("xAI / Grok", "grok"),
        ("Grok Build", "xai"),
        ("GLM / Z.AI", "glm"),
        ("Z.AI", "zai"),
        ("MiniMax", "minimax"),
        ("Kimi", "kimi"),
        ("Amp", "amp"),
    ] {
        assert!(
            provider_matches_usage_label(left, right),
            "{left} should match {right}"
        );
        assert!(
            provider_matches_usage_label(right, left),
            "{right} should match {left}"
        );
    }

    // Different surfaces never match.
    for (left, right) in [
        ("Codex", "claude"),
        ("GLM / Z.AI", "grok"),
        ("Kimi", "minimax"),
    ] {
        assert!(
            !provider_matches_usage_label(left, right),
            "{left} must not match {right}"
        );
    }

    // Providers outside the known surface set (OpenCode) fall through to the
    // case-insensitive substring path — equal labels match, distinct don't.
    assert!(provider_matches_usage_label("OpenCode", "opencode"));
    assert!(!provider_matches_usage_label("OpenCode", "codex"));

    // Unknown text names no surface; the short "amp" token must not match
    // inside an unrelated word (whole-token match, not bare substring), and
    // a glued token ("ampcode") is not a word match either.
    assert_eq!(surface_from_text("totally-unknown"), None);
    assert_eq!(surface_from_text("example"), None);
    assert_eq!(surface_from_text("ampcode"), None);
    assert!(!provider_matches_usage_label("Example", "amp"));
    assert_eq!(surface_from_text("Amp / Code"), Some(UsageSurface::Amp));

    // A known surface never matches an unknown label — this is the
    // production direction (tab label resolves, focus value may not).
    assert!(!provider_matches_usage_label("Codex", "totally-unknown"));
    assert!(!provider_matches_usage_label("Amp", "totally-unknown"));

    // Both unknown → substring fallback: containment matches, distinct don't.
    assert!(provider_matches_usage_label("opencode-zen", "opencode"));
    assert!(!provider_matches_usage_label("opencode", "ollama"));
}

#[test]
fn grok_billing_response_maps_monthly_credits() {
    let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "billingCycle": {
            "billingPeriodStart": "2026-06-01T00:00:00Z",
            "billingPeriodEnd": "2026-07-01T00:00:00Z"
        },
        "monthlyLimit": { "val": 5000 },
        "onDemandCap": { "val": 2500 },
        "on_demand_enabled": true,
        "usage": {
            "includedUsed": { "val": 1500 },
            "onDemandUsed": { "val": 300 },
            "totalUsed": { "val": 1800 }
        }
    }))
    .expect("valid Grok billing response");

    let buckets = usage.buckets(1_780_315_200);

    assert_eq!(buckets[0].label, "Monthly");
    // The RPC/CLI billing path tags its cycle bucket Weekly (no Session); the
    // detail rows must stay untagged so they never reach the headline.
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert!(
        buckets[1..]
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );
    assert_eq!(buckets[0].used_label.as_deref(), Some("$18"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("$50"));
    assert_eq!(buckets[0].remaining_percent, Some(64));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        Some(
            reset_label(
                parse_iso_epoch("2026-07-01T00:00:00Z").expect("billing reset"),
                1_780_315_200,
            )
            .as_str()
        )
    );
    assert_eq!(buckets[0].pace_label, None);
    assert!(
        buckets.iter().any(|bucket| bucket.label == "Included usage"
            && bucket.used_label.as_deref() == Some("$15"))
    );
    assert!(
        buckets
            .iter()
            .any(|bucket| bucket.label == "On-demand usage"
                && bucket.used_label.as_deref() == Some("$3")
                && bucket.limit_label.as_deref() == Some("$25"))
    );
}

#[test]
fn grok_rpc_payload_keeps_billing_method_unescaped() {
    let payload = grok_rpc_request_payload(2, "x.ai/billing", serde_json::json!({}));
    let encoded = serde_json::to_string(&payload).expect("encode payload");

    assert!(encoded.contains("\"method\":\"x.ai/billing\""));
    assert!(!encoded.contains("x.ai\\/billing"));
}

#[test]
fn grok_account_label_prefers_auth_identity_over_env_presence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let auth = dir.path().join("auth.json");
    fs::write(
        &auth,
        r#"{"account":{"email":"operator@example.com"},"token":"redacted"}"#,
    )
    .expect("write auth");

    let label = grok_account_label_or_presence(&auth, true, true, true);

    assert_eq!(label, "operator@example.com");
}

#[test]
fn grok_account_label_reports_safe_credential_presence() {
    let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");

    assert_eq!(
        grok_account_label_or_presence(missing, false, true, true),
        "XAI_API_KEY present"
    );
    assert_eq!(
        grok_account_label_or_presence(missing, false, false, true),
        "GROK_DEPLOYMENT_KEY present"
    );
    assert_eq!(
        grok_account_label_or_presence(missing, false, false, false),
        "needs Grok login"
    );
}

#[test]
fn grok_snapshot_uses_probe_success_without_local_credential_marker() {
    let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");
    let billing: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "billingCycle": {
            "billingPeriodStart": "2026-06-01T00:00:00Z",
            "billingPeriodEnd": "2026-07-01T00:00:00Z"
        },
        "monthlyLimit": { "val": 5000 },
        "usage": { "totalUsed": { "val": 1000 } }
    }))
    .expect("valid Grok billing response");

    let view = grok_snapshot_from_rpc_result(
        "grok",
        1_780_315_200,
        missing,
        false,
        false,
        false,
        Ok(GrokBillingSnapshot::Rpc(billing)),
    );

    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::Cli);
    assert_eq!(view.confidence, UsageConfidence::Authoritative);
    assert_eq!(view.account.account_label, "needs Grok login");
    assert_eq!(view.buckets[0].label, "Monthly");
    assert_eq!(view.buckets[0].remaining_percent, Some(80));
    assert_eq!(view.last_error, None);
}

#[test]
fn grok_web_billing_response_maps_weekly_usage() {
    let data = [
        0x00, 0x00, 0x00, 0x00, 0x3c, 0x0a, 0x3a, 0x0d, 0x9c, 0x7d, 0xac, 0x42, 0x12, 0x00, 0x1a,
        0x00, 0x22, 0x06, 0x08, 0x80, 0x97, 0xf3, 0xd0, 0x06, 0x2a, 0x06, 0x08, 0x80, 0xb1, 0x91,
        0xd2, 0x06, 0x3a, 0x07, 0x08, 0x02, 0x15, 0x12, 0x03, 0xa5, 0x42, 0x42, 0x12, 0x08, 0x01,
        0x12, 0x06, 0x08, 0x80, 0x97, 0xf3, 0xd0, 0x06, 0x1a, 0x06, 0x08, 0x80, 0xb1, 0x91, 0xd2,
        0x06, 0x62, 0x00, 0x68, 0x01, 0x72, 0x00, 0x7a, 0x00, 0x82, 0x01, 0x00, 0x8a, 0x01, 0x00,
        0x92, 0x01, 0x00, 0x9a, 0x01, 0x00, 0xa2, 0x01, 0x00, 0xaa, 0x01, 0x00,
    ];

    let snapshot =
        parse_grok_web_billing_response(&data, 1_782_318_000).expect("parse grok billing");
    let buckets = snapshot.buckets(1_782_318_000);
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));

    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[0].remaining_percent, Some(14));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        Some(
            reset_label(
                parse_iso_epoch("2026-07-01T00:00:00Z").expect("billing reset"),
                1_782_318_000,
            )
            .as_str()
        )
    );
}

#[test]
fn grok_cycle_label_falls_back_to_credits_for_irregular_cycles() {
    assert_eq!(grok_cycle_label_from_minutes(7 * 24 * 60), "Weekly");
    assert_eq!(grok_cycle_label_from_minutes(30 * 24 * 60), "Monthly");
    assert_eq!(grok_cycle_label_from_minutes(13 * 24 * 60), "Credits");
}

#[test]
fn grok_snapshot_reports_probe_error_instead_of_presence_gate() {
    let missing = Path::new("/tmp/nonexistent-grok-auth-for-test.json");

    let view = grok_snapshot_from_rpc_result(
        "grok",
        1_780_315_200,
        missing,
        false,
        false,
        false,
        Err("grok agent stdio failed to start: not found".to_owned()),
    );

    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert_eq!(
        view.last_error.as_deref(),
        Some("grok agent stdio failed to start: not found")
    );
}

#[test]
fn codex_oauth_credentials_parse_nested_tokens() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("auth.json");
    let id_token = test_jwt(serde_json::json!({
        "email": "person@example.com",
        "sub": "acct-sub"
    }));
    fs::write(
        &path,
        serde_json::json!({
            "tokens": {
                "access_token": "access",
                "refresh_token": "refresh",
                "account_id": "acct",
                "id_token": id_token
            }
        })
        .to_string(),
    )
    .expect("write auth");

    let credentials = load_codex_oauth_credentials(&path).expect("credentials");

    assert_eq!(credentials.access_token, "access");
    assert_eq!(credentials.account_id.as_deref(), Some("acct"));
    assert_eq!(
        credentials.account_label.as_deref(),
        Some("person@example.com")
    );
}

#[test]
fn codex_id_token_identity_falls_back_to_subject() {
    let id_token = test_jwt(serde_json::json!({
        "sub": "user-123"
    }));

    assert_eq!(
        codex_account_label_from_id_token(&id_token).as_deref(),
        Some("ChatGPT account user-123")
    );
}

#[test]
fn credential_file_loaders_reread_updated_container_files() {
    let dir = tempfile::tempdir().expect("tempdir");

    let claude_path = dir.path().join(".credentials.json");
    fs::write(
        &claude_path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "old-claude",
                "subscriptionType": "max"
            }
        })
        .to_string(),
    )
    .expect("write Claude auth");
    assert_eq!(
        load_claude_oauth_credentials(&claude_path)
            .expect("Claude credentials")
            .access_token,
        "old-claude"
    );
    fs::write(
        &claude_path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "new-claude",
                "subscriptionType": "max"
            }
        })
        .to_string(),
    )
    .expect("refresh Claude auth");
    assert_eq!(
        load_claude_oauth_credentials(&claude_path)
            .expect("updated Claude credentials")
            .access_token,
        "new-claude"
    );

    let codex_path = dir.path().join("auth.json");
    fs::write(
        &codex_path,
        serde_json::json!({
            "tokens": {
                "access_token": "old-codex",
                "id_token": test_jwt(serde_json::json!({"email": "old@example.com"}))
            }
        })
        .to_string(),
    )
    .expect("write Codex auth");
    assert_eq!(
        load_codex_oauth_credentials(&codex_path)
            .expect("Codex credentials")
            .access_token,
        "old-codex"
    );
    fs::write(
        &codex_path,
        serde_json::json!({
            "tokens": {
                "access_token": "new-codex",
                "id_token": test_jwt(serde_json::json!({"email": "new@example.com"}))
            }
        })
        .to_string(),
    )
    .expect("refresh Codex auth");
    let codex = load_codex_oauth_credentials(&codex_path).expect("updated Codex credentials");
    assert_eq!(codex.access_token, "new-codex");
    assert_eq!(codex.account_label.as_deref(), Some("new@example.com"));

    let kimi_path = dir.path().join(".kimi-code/credentials/kimi-code.json");
    fs::create_dir_all(kimi_path.parent().expect("Kimi credentials parent"))
        .expect("create Kimi credentials dir");
    fs::write(
        &kimi_path,
        serde_json::json!({
            "access_token": "old-kimi",
            "expires_at": 1_781_300_000
        })
        .to_string(),
    )
    .expect("write Kimi auth");
    assert_eq!(
        load_kimi_local_token_from_home(dir.path(), 1_781_200_000).as_deref(),
        Some("old-kimi")
    );
    fs::write(
        &kimi_path,
        serde_json::json!({
            "access_token": "new-kimi",
            "expires_at": 1_781_300_000
        })
        .to_string(),
    )
    .expect("refresh Kimi auth");
    assert_eq!(
        load_kimi_local_token_from_home(dir.path(), 1_781_200_000).as_deref(),
        Some("new-kimi")
    );
    fs::write(
        &kimi_path,
        serde_json::json!({
            "access_token": "expired-kimi",
            "expires_at": 1_781_100_000
        })
        .to_string(),
    )
    .expect("expire Kimi auth");
    assert_eq!(
        load_kimi_local_token_from_home(dir.path(), 1_781_200_000),
        None
    );
}

fn test_jwt(payload: serde_json::Value) -> String {
    let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("{}");
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string());
    format!("{header}.{payload}.signature")
}

#[test]
fn quota_pace_label_uses_codexbar_reserve_deficit_onpace() {
    // Behind pace (burning faster than the clock): 60% quota left with 90%
    // of the window still remaining -> 30 points of deficit.
    let deficit = quota_pace_label(Some(60), Some(900), Some(1_000), 0).expect("pace label");
    assert_eq!(deficit, "30% in deficit");

    // Ahead of pace (quota outlasting the clock): 90% left, 60% of window
    // remaining -> 30 points in reserve.
    let reserve = quota_pace_label(Some(90), Some(600), Some(1_000), 0).expect("pace label");
    assert_eq!(reserve, "30% in reserve");

    // Within 2 points of the clock -> On pace.
    let on_pace = quota_pace_label(Some(50), Some(500), Some(1_000), 0).expect("pace label");
    assert_eq!(on_pace, "On pace");
}

#[test]
fn reset_label_uses_relative_and_local_timestamp() {
    let now = parse_iso_epoch("2026-06-11T13:46:00Z").expect("now");
    let same_day = parse_iso_epoch("2026-06-11T15:12:00Z").expect("same day");
    assert_eq!(
        reset_label(same_day, now),
        format!("Resets in 1h 26m ({})", local_timestamp_label(same_day))
    );
    let tomorrow = parse_iso_epoch("2026-06-12T04:18:00Z").expect("tomorrow");
    assert_eq!(
        reset_label(tomorrow, now),
        format!("Resets in 14h 32m ({})", local_timestamp_label(tomorrow))
    );
    let future = parse_iso_epoch("2026-07-01T16:31:00Z").expect("future");
    assert_eq!(
        reset_label(future, now),
        format!("Resets in 20d 2h ({})", local_timestamp_label(future))
    );
    assert_eq!(reset_label(now, now), "Resets now");
}

#[test]
fn claude_oauth_credentials_parse_subscription_label() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "access",
                "subscriptionType": "claude_max"
            }
        })
        .to_string(),
    )
    .expect("write auth");

    let credentials = load_claude_oauth_credentials(&path).expect("credentials");

    assert_eq!(credentials.access_token, "access");
    assert_eq!(credentials.subscription_type.as_deref(), Some("Claude Max"));
}

#[test]
fn claude_oauth_credentials_fall_back_to_rate_limit_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "claudeAiOauth": {
                "accessToken": "access",
                "rateLimitTier": "max"
            }
        })
        .to_string(),
    )
    .expect("write auth");

    let credentials = load_claude_oauth_credentials(&path).expect("credentials");

    assert_eq!(credentials.access_token, "access");
    assert_eq!(credentials.subscription_type.as_deref(), Some("Max"));
}

#[test]
fn claude_organization_type_humanizes_enterprise_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "oauthAccount": {
                "emailAddress": "user@company.com",
                "organizationType": "claude_enterprise",
                "subscriptionType": "API Usage Billing"
            }
        })
        .to_string(),
    )
    .expect("write account");
    assert_eq!(
        load_claude_organization_type(&path).as_deref(),
        Some("Claude Enterprise")
    );
}

#[test]
fn claude_organization_type_humanizes_team_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "oauthAccount": {
                "emailAddress": "user@team.ai",
                "organizationType": "claude_team"
            }
        })
        .to_string(),
    )
    .expect("write account");
    assert_eq!(
        load_claude_organization_type(&path).as_deref(),
        Some("Claude Team")
    );
}

#[test]
fn claude_organization_type_humanizes_max_tier() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({
            "oauthAccount": {
                "organizationType": "claude_max"
            }
        })
        .to_string(),
    )
    .expect("write account");
    assert_eq!(
        load_claude_organization_type(&path).as_deref(),
        Some("Claude Max")
    );
}

#[test]
fn claude_organization_type_absent_returns_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("claude.json");
    fs::write(
        &path,
        serde_json::json!({ "oauthAccount": { "emailAddress": "x@y.com" } }).to_string(),
    )
    .expect("write account");
    assert_eq!(load_claude_organization_type(&path), None);
}

#[test]
fn claude_code_user_agent_parses_cli_version() {
    assert_eq!(
        claude_code_version_from_text("Claude Code 2.1.7\n").as_deref(),
        Some("2.1.7")
    );
    assert_eq!(
        claude_code_user_agent_with(|command, args, timeout| {
            assert_eq!(command, "claude");
            assert_eq!(args, ["--version"]);
            assert_eq!(timeout, CLAUDE_VERSION_TIMEOUT);
            Ok(CliOutput {
                success: true,
                exit_code: Some(0),
                stdout: "Claude Code 2.2.0".to_owned(),
                stderr: String::new(),
            })
        })
        .as_deref(),
        Some("claude-code/2.2.0")
    );
}

#[test]
fn amp_cli_usage_parser_maps_free_and_credit_rows() {
    let usage = parse_amp_usage_output(
            "Signed in as person@example.com (handle)\n\
             Amp Free: $2.42/$10 remaining (replenishes +$0.42/hour) - https://ampcode.com/settings#amp-free\n\
             Individual credits: $0.33 remaining (set up automatic top-up to avoid running out)\n",
        )
        .expect("Amp usage");

    assert_eq!(
        usage.account_label.as_deref(),
        Some("person@example.com (handle)")
    );
    let now = 1_781_185_560;
    let buckets = usage.buckets(now);
    assert_eq!(buckets[0].label, "Amp Free");
    assert_eq!(buckets[0].used_label.as_deref(), Some("$7.58"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("$10"));
    assert_eq!(buckets[0].remaining_percent, Some(24));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        amp_free_reset_label(2.42, 10.0, Some(0.42), now).as_deref()
    );
    assert_eq!(buckets[0].pace_label, None);
    assert_eq!(buckets[1].label, "Individual credits");
    assert_eq!(buckets[1].limit_label.as_deref(), Some("$0.33"));
    assert_eq!(
        buckets[1].pace_label.as_deref(),
        Some("Individual credits: $0.33")
    );
}

#[test]
fn cli_output_collector_treats_reaped_child_as_success() {
    let output = collect_cli_output(
        "amp",
        None,
        thread::spawn(|| Ok("usage rows".to_owned())),
        thread::spawn(|| Ok(String::new())),
    )
    .expect("cli output");

    assert!(output.success);
    assert_eq!(output.exit_code, None);
    assert_eq!(output.stdout, "usage rows");
}

#[test]
fn amp_api_usage_maps_display_balance_info() {
    let usage = AmpApiUsage::from_value(serde_json::json!({
        "result": {
            "user": { "email": "person@example.com" },
            "ampFree": {
                "ampFreeRemaining": 4.94,
                "ampFreeLimit": 10.0,
                "hourlyReplenishment": 0.42
            },
            "credits": {
                "individualCredits": 1.25
            }
        }
    }))
    .expect("Amp API usage");

    assert_eq!(usage.account_label.as_deref(), Some("person@example.com"));
    let now = 1_781_185_560;
    let buckets = usage.buckets(now);
    assert_eq!(buckets[0].label, "Amp Free");
    assert_eq!(buckets[0].used_label.as_deref(), Some("$5.06"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("$10"));
    assert_eq!(buckets[0].remaining_percent, Some(49));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        amp_free_reset_label(4.94, 10.0, Some(0.42), now).as_deref()
    );
    assert_eq!(buckets[0].pace_label, None);
    assert_eq!(buckets[1].label, "Individual credits");
    assert_eq!(buckets[1].limit_label.as_deref(), Some("$1.25"));
    assert_eq!(
        buckets[1].pace_label.as_deref(),
        Some("Individual credits: $1.25")
    );
}

#[test]
fn amp_api_usage_maps_display_text_response() {
    let usage = AmpApiUsage::from_value(serde_json::json!({
            "ok": true,
            "result": {
                "displayText": "Signed in as person@example.com (handle)\n\
                    Amp Free: $7.17/$10 remaining (replenishes +$0.42/hour) - https://ampcode.com/settings#amp-free\n\
                    Individual credits: $4.76 remaining - https://ampcode.com/settings"
            }
        }))
        .expect("Amp API display text usage");

    assert_eq!(
        usage.account_label.as_deref(),
        Some("person@example.com (handle)")
    );
    let now = 1_781_185_560;
    let buckets = usage.buckets(now);
    assert_eq!(buckets[0].label, "Amp Free");
    assert_eq!(buckets[0].used_label.as_deref(), Some("$2.83"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("$10"));
    assert_eq!(buckets[0].remaining_percent, Some(72));
    assert_eq!(
        buckets[0].reset_label.as_deref(),
        amp_free_reset_label(7.17, 10.0, Some(0.42), now).as_deref()
    );
    assert_eq!(buckets[1].label, "Individual credits");
    assert_eq!(buckets[1].limit_label.as_deref(), Some("$4.76"));
}

#[test]
fn amp_secrets_json_provides_api_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("secrets.json");
    fs::write(
        &path,
        serde_json::json!({
            "other": "ignored",
            "apiKey@https://ampcode.com/": " amp-token "
        })
        .to_string(),
    )
    .expect("write Amp secrets");

    assert_eq!(load_amp_api_key(&path).as_deref(), Some("amp-token"));
}

#[test]
fn zai_quota_response_maps_token_session_and_time_limits() {
    let quota: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "msg": "ok",
        "data": {
            "planName": "Coding Pro",
            "limits": [
                {
                    "type": "TOKENS_LIMIT",
                    "unit": 5,
                    "number": 300,
                    "usage": 1000,
                    "currentValue": 250,
                    "remaining": 750,
                    "percentage": 25,
                    "nextResetTime": 1_781_189_520_000_i64
                },
                {
                    "type": "TOKENS_LIMIT",
                    "unit": 6,
                    "number": 1,
                    "usage": 10000,
                    "currentValue": 9000,
                    "remaining": 1000,
                    "percentage": 90,
                    "nextResetTime": 1_781_798_400_000_i64
                },
                {
                    "type": "TIME_LIMIT",
                    "unit": 5,
                    "number": 1,
                    "usage": 120,
                    "currentValue": 30,
                    "remaining": 90,
                    "percentage": 25
                }
            ]
        }
    }))
    .expect("valid Z.AI quota");

    let buckets = quota.buckets(1_781_185_560);

    assert_eq!(quota.plan_name().as_deref(), Some("Coding Pro"));
    // render order is 5-hour, Tokens, MCP.
    assert_eq!(buckets[0].label, "5-hour");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].remaining_percent, Some(75));
    assert_eq!(buckets[0].pace_label, None);
    assert_eq!(buckets[1].label, "Tokens");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].remaining_percent, Some(10));
    assert_eq!(buckets[1].pace_label, None);
    assert_eq!(buckets[2].label, "MCP");
    assert_eq!(buckets[2].status_slot, None);
    assert_eq!(buckets[2].remaining_percent, Some(75));
    assert_eq!(
        buckets[2].pace_label.as_deref(),
        Some("30 / 120 (90 remaining)")
    );
}

#[test]
fn zai_url_normalization_accepts_hosts_and_full_urls() {
    assert_eq!(
        normalize_url_or_host("open.bigmodel.cn", "api/monitor/usage/quota/limit"),
        "https://open.bigmodel.cn/api/monitor/usage/quota/limit"
    );
    assert_eq!(
        normalize_url_or_host("https://example.test/custom", ""),
        "https://example.test/custom"
    );
    assert_eq!(
        normalize_url_or_host(
            &zai_quota_host("https://api.z.ai/api/anthropic"),
            "api/monitor/usage/quota/limit"
        ),
        "https://api.z.ai/api/monitor/usage/quota/limit"
    );
    assert_eq!(
        resolve_zai_quota_url_from(Some("https://example.test/quota"), None),
        "https://example.test/quota"
    );
}

#[test]
fn kimi_usage_response_maps_weekly_and_rate_limit() {
    let usage: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "usages": [{
            "scope": "FEATURE_CODING",
            "detail": {
                "limit": "1000",
                "used": "220",
                "remaining": "780",
                "resetTime": "2026-06-18T12:00:00Z"
            },
            "limits": [{
                "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                "detail": {
                    "limit": "200",
                    "remaining": "150",
                    "resetTime": "2026-06-11T16:00:00Z"
                }
            }]
        }]
    }))
    .expect("valid Kimi usage");

    let buckets = usage.buckets(1_781_185_560);

    // render order is Rate Limit, then Weekly.
    assert_eq!(buckets[0].label, "Rate Limit");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Session));
    assert_eq!(buckets[0].used_label.as_deref(), Some("50"));
    assert_eq!(buckets[0].remaining_percent, Some(75));
    assert_eq!(buckets[0].pace_label.as_deref(), Some("30% in reserve"));
    assert_eq!(buckets[1].label, "Weekly");
    assert_eq!(buckets[1].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[1].used_label.as_deref(), Some("220"));
    assert_eq!(buckets[1].limit_label.as_deref(), Some("1.0K"));
    assert_eq!(buckets[1].remaining_percent, Some(78));
    assert_eq!(buckets[1].pace_label, None);
}

#[test]
fn kimi_local_token_loader_skips_expired_tokens() {
    let value = serde_json::json!({
        "access_token": "expired-token",
        "expires_at": 1_781_000_000.0
    });

    assert_eq!(kimi_local_token_from_value(&value, 1_781_200_000), None);
}

#[test]
fn kimi_local_token_loader_accepts_unexpired_tokens() {
    let value = serde_json::json!({
        "access_token": "fresh-token",
        "expires_at": 1_781_300_000
    });

    assert_eq!(
        kimi_local_token_from_value(&value, 1_781_200_000).as_deref(),
        Some("fresh-token")
    );
}

#[test]
fn kimi_local_token_loader_normalizes_millisecond_expiry() {
    let value = serde_json::json!({
        "access_token": "fresh-ms-token",
        "expires_at": 1_781_300_000_000_i64
    });

    assert_eq!(
        kimi_local_token_from_value(&value, 1_781_200_000).as_deref(),
        Some("fresh-ms-token")
    );
}

#[test]
fn minimax_usage_response_maps_model_remains() {
    let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
        "base_resp": { "status_code": 0 },
        "data": {
            "current_subscribe_title": "MiniMax Pro",
            "model_remains": [{
                "model_name": "MiniMax Text",
                "current_interval_total_count": 100,
                "current_interval_usage_count": 60,
                "current_interval_status": 0,
                "start_time": 1781172000,
                "end_time": 1781186400,
                "current_weekly_total_count": 700,
                "current_weekly_usage_count": 630,
                "current_weekly_remaining_percent": 90,
                "weekly_start_time": 1780761600,
                "weekly_end_time": 1781366400
            }]
        }
    }))
    .expect("valid MiniMax usage");

    usage.validate().expect("valid quota response");
    let buckets = usage.buckets(1_781_185_560);

    assert_eq!(usage.plan_name().as_deref(), Some("MiniMax Pro"));
    assert_eq!(buckets[0].label, "MiniMax Text");
    // A non-general model fills no headline slot.
    assert_eq!(buckets[0].status_slot, None);
    assert_eq!(buckets[0].used_label.as_deref(), Some("60"));
    assert_eq!(buckets[0].limit_label.as_deref(), Some("100"));
    assert_eq!(buckets[0].remaining_percent, Some(40));
    assert_eq!(buckets[0].pace_label.as_deref(), Some("Usage: 60 / 100"));
    assert_eq!(buckets.len(), 1);
}

#[test]
fn minimax_usage_response_maps_live_root_model_remains() {
    let usage: MiniMaxUsageResponse = serde_json::from_value(serde_json::json!({
        "model_remains": [
            {
                "model_name": "general",
                "current_interval_total_count": 0,
                "current_interval_usage_count": 0,
                "current_interval_remaining_percent": 100,
                "current_interval_status": 1,
                "remains_time": 14_400_000,
                "current_weekly_total_count": 0,
                "current_weekly_usage_count": 1,
                "current_weekly_remaining_percent": 99,
                "current_weekly_status": 1,
                "weekly_remains_time": 345_600_000
            },
            {
                "model_name": "video",
                "current_interval_total_count": 5,
                "current_interval_usage_count": 0,
                "current_interval_remaining_percent": 100,
                "current_interval_status": 1,
                "remains_time": 28_800_000,
                "current_weekly_total_count": 35,
                "current_weekly_usage_count": 0,
                "current_weekly_remaining_percent": 100,
                "current_weekly_status": 1,
                "weekly_remains_time": 345_600_000
            }
        ],
        "base_resp": { "status_code": 0, "status_msg": "success" }
    }))
    .expect("valid MiniMax usage");

    usage.validate().expect("valid quota response");
    let buckets = usage.buckets(1_782_315_600);

    assert_eq!(
        buckets
            .iter()
            .map(|bucket| {
                (
                    bucket.label.as_str(),
                    bucket.remaining_percent,
                    bucket.pace_label.as_deref(),
                )
            })
            .collect::<Vec<_>>(),
        vec![
            ("General · 5h", Some(100), Some("Usage: 0 / 100")),
            ("General · Weekly", Some(99), Some("Usage: 1 / 100")),
            ("Video", Some(100), Some("Usage: 0 / 5")),
        ]
    );
    // The general model's windows fill the headline slots; per-model windows
    // (Video) fill none.
    assert_eq!(
        buckets
            .iter()
            .map(|bucket| (bucket.label.as_str(), bucket.status_slot))
            .collect::<Vec<_>>(),
        vec![
            ("General · 5h", Some(StatusSlot::Session)),
            ("General · Weekly", Some(StatusSlot::Weekly)),
            ("Video", None),
        ]
    );
}

#[test]
fn minimax_remains_urls_accept_override_and_api_host_alias() {
    assert_eq!(
        resolve_minimax_remains_urls_from(Some("https://example.test/custom"), None),
        vec!["https://example.test/custom"]
    );

    assert_eq!(
        resolve_minimax_remains_urls_from(None, Some("https://api.minimax.io/anthropic")),
        vec![
            "https://api.minimax.io/v1/token_plan/remains",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains"
        ]
    );
}

#[test]
fn usage_surface_synonyms_are_lowercase() {
    // surface_from_text lowercases the haystack before comparing, so any
    // uppercase synonym entry would be permanently unmatchable.
    for surface in UsageSurface::ALL {
        for syn in surface.synonyms() {
            assert_eq!(
                *syn,
                syn.to_ascii_lowercase(),
                "synonym {syn:?} for {surface:?} must be lowercase"
            );
        }
    }
}

#[test]
fn usage_surface_all_lists_every_variant() {
    // `guard` has no wildcard arm: adding a UsageSurface variant makes it fail to
    // compile, forcing the author to this test. `variants` then drives the runtime
    // check that each variant is present in ALL — a variant missing from ALL is
    // silently unmatchable in surface_from_text. The len check catches the reverse.
    fn guard(surface: UsageSurface) {
        match surface {
            UsageSurface::Claude
            | UsageSurface::Codex
            | UsageSurface::Amp
            | UsageSurface::Grok
            | UsageSurface::Zai
            | UsageSurface::Kimi
            | UsageSurface::Minimax
            | UsageSurface::OpenCode
            | UsageSurface::Unsupported => {}
        }
    }
    let variants = [
        UsageSurface::Claude,
        UsageSurface::Codex,
        UsageSurface::Amp,
        UsageSurface::Grok,
        UsageSurface::Zai,
        UsageSurface::Kimi,
        UsageSurface::Minimax,
        UsageSurface::OpenCode,
        UsageSurface::Unsupported,
    ];
    for surface in variants {
        guard(surface);
        assert!(
            UsageSurface::ALL.contains(&surface),
            "{surface:?} missing from UsageSurface::ALL"
        );
    }
    assert_eq!(
        UsageSurface::ALL.len(),
        variants.len(),
        "UsageSurface::ALL has an entry this test does not cover"
    );
}

#[test]
fn provider_outcome_maps_presence_states() {
    use jackin_protocol::control::{UsageConfidence, UsageSnapshotStatus, UsageSource};
    assert_eq!(
        provider_outcome(ProviderPresence {
            has_data: true,
            has_secret: true
        }),
        (
            UsageSnapshotStatus::Fresh,
            UsageSource::ProviderApi,
            UsageConfidence::Authoritative
        )
    );
    assert_eq!(
        provider_outcome(ProviderPresence {
            has_data: false,
            has_secret: true
        }),
        (
            UsageSnapshotStatus::Unsupported,
            UsageSource::None,
            UsageConfidence::PresenceOnly
        )
    );
    assert_eq!(
        provider_outcome(ProviderPresence {
            has_data: false,
            has_secret: false
        }),
        (
            UsageSnapshotStatus::NeedsSecret,
            UsageSource::None,
            UsageConfidence::None
        )
    );
}

#[test]
fn split_fetch_partitions_ok_err_and_absent() {
    assert_eq!(split_fetch(Some(Ok::<_, String>(7u64))), (Some(7), None));
    assert_eq!(
        split_fetch(Some(Err::<u64, _>("boom".to_owned()))),
        (None, Some("boom".to_owned()))
    );
    assert_eq!(split_fetch(None::<Result<u64, String>>), (None, None));
}
