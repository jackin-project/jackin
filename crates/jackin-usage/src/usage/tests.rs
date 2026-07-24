// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::thread;

#[test]
fn compact_count_uses_token_suffixes() {
    assert_eq!(compact_count(999), "999");
    assert_eq!(compact_count(1_500), "1.5K");
    assert_eq!(compact_count(2_000_000), "2.0M");
}

#[test]
fn provider_connector_exports_physical_attempts_without_endpoint_material() {
    use std::io::{Read as _, Write as _};

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _read = stream.read(&mut request).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok")
            .unwrap();
    });
    let secret_route = "provider-secret-route?token=provider-secret-query";
    provider_http_client()
        .unwrap()
        .get(format!("http://{address}/{secret_route}"))
        .send()
        .unwrap();
    server.join().unwrap();

    let refused = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let refused_address = refused.local_addr().unwrap();
    drop(refused);
    provider_http_client()
        .unwrap()
        .get(format!("http://{refused_address}/{secret_route}"))
        .send()
        .unwrap_err();

    export.force_flush();
    let spans = export.finished_spans();
    assert_eq!(spans.len(), 2);
    assert!(
        spans
            .iter()
            .all(|span| span.name == jackin_telemetry::schema::spans::CONNECTION_ATTEMPT)
    );
    assert_eq!(export.error_span_count(), 1);
    assert!(export.contains_span_text("provider"));
    assert!(export.contains_span_text("error"));
    assert!(export.contains_span_text("io_error"));
    for prohibited in [
        secret_route,
        "provider-secret-query",
        &address.to_string(),
        &refused_address.to_string(),
    ] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
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
    cache.set_usage_snapshot_store_path(dir.path().join("missing").join("snapshots.db"));
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
    cache.set_usage_snapshot_store_path(dir.path().join("missing").join("snapshots.db"));
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
    let db = dir.path().join("snapshots.db");
    crate::usage_snapshot_store::store_usage_snapshot(&db, &codex_cached_usage_view())
        .expect("store usage snapshot");
    let mut cache = UsageCache::default();
    cache.set_usage_snapshot_store_path(db);

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
    let db = dir.path().join("snapshots.db");
    crate::usage_snapshot_store::store_usage_snapshot(&db, &codex_cached_usage_view())
        .expect("store usage snapshot");
    let mut cache = UsageCache::default();
    cache.set_usage_snapshot_store_path(db);

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
fn usage_refresh_max_active_probes_are_spawned_before_any_join() {
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::{Arc, Condvar, Mutex};

    struct Rendezvous {
        state: Mutex<RendezvousState>,
        changed: Condvar,
    }

    struct RendezvousState {
        entered: usize,
        released: bool,
    }

    impl Rendezvous {
        fn wait_for_two(&self) {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.entered += 1;
            if state.entered == 2 {
                state.released = true;
                self.changed.notify_all();
                return;
            }
            while !state.released {
                let (next_state, wait) = self
                    .changed
                    .wait_timeout(state, Duration::from_secs(1))
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state = next_state;
                assert!(
                    !wait.timed_out() || state.released,
                    "second refresh probe never overlapped with the first"
                );
            }
        }
    }

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
    let rendezvous = Arc::new(Rendezvous {
        state: Mutex::new(RendezvousState {
            entered: 0,
            released: false,
        }),
        changed: Condvar::new(),
    });

    let results = collect_usage_refresh_results_with_timeout(
        targets,
        {
            let active = Arc::clone(&active);
            let max_active = Arc::clone(&max_active);
            let rendezvous = Arc::clone(&rendezvous);
            move |target| {
                let now_active = active.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                max_active.fetch_max(now_active, AtomicOrdering::SeqCst);
                rendezvous.wait_for_two();
                active.fetch_sub(1, AtomicOrdering::SeqCst);
                UsageRefreshResult {
                    target,
                    view: FocusedUsageView::unavailable("test", now_epoch()),
                    policy: UsageSnapshotPolicy::Shared,
                    codex_rpc_gate: ManagedCliLaunchGate::default(),
                    grok_rpc_gate: ManagedCliLaunchGate::default(),
                }
            }
        },
        Duration::from_secs(2),
    );

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
                policy: UsageSnapshotPolicy::Shared,
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

    // Shared cooldown is account-scoped (Class III); assert with the same key
    // the production write path uses.
    let key = target.shared_account_key();
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

    // Shared files are account-scoped (Class III); use the same key the
    // production write path uses so the round-trip is exercised faithfully.
    let key = target.shared_account_key();
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
fn shared_usage_dirs_default_under_usage_shared() {
    // Env overrides are for tests only; with them unset, defaults land under
    // ~/.jackin/data/usage-shared/{cooldowns,snapshots,locks}.
    let cases = [
        (
            "JACKIN_USAGE_COOLDOWN_DIR",
            shared_usage_cooldown_dir(),
            "data/usage-shared/cooldowns",
        ),
        (
            "JACKIN_USAGE_SNAPSHOTS_DIR",
            shared_usage_snapshots_dir(),
            "data/usage-shared/snapshots",
        ),
        (
            "JACKIN_USAGE_LOCK_DIR",
            shared_usage_lock_dir(),
            "data/usage-shared/locks",
        ),
    ];
    for (var, path, suffix) in cases {
        // Safety: only assert the default path when the override is unset in
        // this process; parallel tests that set the var would change meaning.
        if std::env::var_os(var).is_some() {
            continue;
        }
        let path_str = path.to_string_lossy().replace('\\', "/");
        assert!(
            path_str.ends_with(suffix),
            "{var} default should end with {suffix}, got {path_str}"
        );
        assert!(
            !path_str.contains("daemon/usage-"),
            "{var} must not use legacy daemon/usage-* defaults, got {path_str}"
        );
    }
}

#[test]
fn adopt_shared_snapshots_reseeds_when_shared_is_newer() {
    let snapshots_dir = tempfile::tempdir().expect("tempdir");
    let target = UsageRefreshTarget {
        agent: "claude".to_owned(),
        provider: None,
    };
    let account_key = target.shared_account_key();
    let cache_key = target.cache_key();

    let older = FocusedUsageView::unavailable("older", 1_000);
    let newer = FocusedUsageView::unavailable("newer", 2_000);
    write_shared_usage_snapshot(snapshots_dir.path(), &account_key, &newer);

    let mut cache_b = UsageCache::default();
    cache_b
        .snapshots
        .insert(cache_key.clone(), CachedUsage { view: older });
    cache_b.adopt_shared_snapshots(std::slice::from_ref(&target), snapshots_dir.path());

    let adopted = cache_b
        .snapshots
        .get(&cache_key)
        .expect("occupied entry remains");
    assert_eq!(
        adopted.view.fetched_at_epoch, 2_000,
        "warm cache must adopt strictly newer shared snapshot"
    );
    assert_eq!(adopted.view.status, UsageSnapshotStatus::Stale);
    assert_eq!(adopted.view.source, UsageSource::Cache);
    // Identity scoping: a different account key's file is not read for this target.
    let other_key = "other-account-key";
    let alien = FocusedUsageView::unavailable("alien", 9_999);
    write_shared_usage_snapshot(snapshots_dir.path(), other_key, &alien);
    cache_b.adopt_shared_snapshots(std::slice::from_ref(&target), snapshots_dir.path());
    assert_eq!(
        cache_b
            .snapshots
            .get(&cache_key)
            .expect("still present")
            .view
            .fetched_at_epoch,
        2_000,
        "foreign account key must not replace this surface's view"
    );
}

#[test]
fn adopt_shared_snapshots_mtime_guard_skips_json_reread() {
    let snapshots_dir = tempfile::tempdir().expect("tempdir");
    let target = UsageRefreshTarget {
        agent: "claude".to_owned(),
        provider: None,
    };
    let account_key = target.shared_account_key();
    let view = FocusedUsageView::unavailable("seed", 1_500);
    write_shared_usage_snapshot(snapshots_dir.path(), &account_key, &view);

    let mut cache = UsageCache::default();
    cache.adopt_shared_snapshots(std::slice::from_ref(&target), snapshots_dir.path());
    assert_eq!(cache.shared_snapshot_json_reads, 1);
    cache.adopt_shared_snapshots(std::slice::from_ref(&target), snapshots_dir.path());
    assert_eq!(
        cache.shared_snapshot_json_reads, 1,
        "unchanged mtime must not re-parse shared snapshot JSON"
    );
}

#[test]
fn success_cooldown_suppresses_due_target_but_force_still_refreshes() {
    let cooldown_dir = tempfile::tempdir().expect("tempdir");
    let snapshots_dir = tempfile::tempdir().expect("tempdir");
    let target = UsageRefreshTarget {
        agent: "claude".to_owned(),
        provider: None,
    };
    let now = Instant::now();
    let view = FocusedUsageView::unavailable("from-a", now_epoch());

    // Process A: successful refresh writes success cooldown + snapshot.
    let mut schedule_a = UsageRefreshSchedule::default();
    assert!(schedule_a.should_refresh_with_cooldown_dir(&target, now, cooldown_dir.path()));
    schedule_a.mark_refreshed_with_cooldown_dir(
        &target,
        now,
        &view,
        cooldown_dir.path(),
        snapshots_dir.path(),
    );

    // Process B: warm schedule with the target already due (timer), no force.
    let mut schedule_b = UsageRefreshSchedule::default();
    let due_past = now
        .checked_sub(Duration::from_secs(1))
        .expect("instant subtract");
    schedule_b.next_due.insert(target.cache_key(), due_past);
    assert!(
        !schedule_b.should_refresh_with_cooldown_dir(&target, now, cooldown_dir.path()),
        "due target must skip probe while another process's success cooldown is active"
    );

    // Forced refresh (menu bar / request_account_refresh) still probes.
    schedule_b.mark_due(&target, now);
    assert!(
        schedule_b.should_refresh_with_cooldown_dir(&target, now, cooldown_dir.path()),
        "force refresh must bypass success cooldown"
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
    // Sonnet is a weekly window, so the unified model paces it the same way a
    // `limits`-sourced Fable window is paced (it has both a reset and a 7-day
    // duration). Daily Routines carries no `resets_at`, so it still has none.
    assert!(
        buckets
            .iter()
            .find(|bucket| bucket.label == "Sonnet")
            .is_some_and(|bucket| bucket.pace_label.is_some())
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

/// The `limits` array is the authoritative shape on current accounts: it
/// carries Session, "All models" Weekly, and per-model Weekly (`weekly_scoped`
/// — Fable today). When present, `into_buckets` builds from it and must NOT
/// also emit the legacy `seven_day*` windows (the API returns both, so
/// skipping the legacy path is what prevents double rows). Mirrors the live
/// 2026-07-03 OAuth response: session 7%, all-models 28%, Fable 35%.
#[test]
fn claude_oauth_limits_array_surfaces_fable_and_all_models() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        // Legacy named windows are still present but null on current accounts;
        // they must contribute nothing because `limits` takes precedence.
        "five_hour": null,
        "seven_day": null,
        "seven_day_sonnet": null,
        "seven_day_opus": null,
        "seven_day_cowork": null,
        "limits": [
            { "kind": "session", "group": "session", "percent": 7,
              "severity": "normal", "resets_at": "2026-07-03T03:19:59Z",
              "scope": null, "is_active": false },
            { "kind": "weekly_all", "group": "weekly", "percent": 28,
              "severity": "normal", "resets_at": "2026-07-03T07:00:00Z",
              "scope": null, "is_active": false },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 35,
              "severity": "warn", "resets_at": "2026-07-03T06:59:59Z",
              "scope": { "model": { "id": null, "display_name": "Fable" },
                         "surface": null },
              "is_active": true }
        ]
    }))
    .expect("valid Claude OAuth limits-array response");

    let buckets = usage.into_buckets(1_781_300_000);

    let session = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Session))
        .expect("session bucket from limits");
    assert_eq!(session.label, "Session");
    assert_eq!(session.remaining_percent, Some(93));
    assert_eq!(session.used_label.as_deref(), Some("7% used"));

    // "All models" fills the Weekly headline slot (status bar still reads
    // "Weekly" via the slot), label matches the web console row.
    let all_models = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly slot from limits");
    assert_eq!(all_models.label, "All models");
    assert_eq!(all_models.remaining_percent, Some(72));

    // Fable — the model-scoped window the legacy parser dropped. Non-headline
    // (no status slot), severity mirrored from the API for meter color.
    let fable = buckets
        .iter()
        .find(|b| b.label == "Fable")
        .expect("Fable model-scoped bucket");
    assert_eq!(fable.remaining_percent, Some(65));
    assert_eq!(fable.used_label.as_deref(), Some("35% used"));
    assert_eq!(fable.status_slot, None);
    assert_eq!(fable.severity, UsageSeverity::Warn);
    // Reset epoch is carried (RC2) so the CLI report can emit `resets_at`.
    assert!(fable.resets_at.is_some());

    // No legacy fabricated rows leaked through: the null `seven_day*` windows
    // produce nothing once `limits` is authoritative.
    assert!(buckets.iter().all(|b| b.label != "Weekly"));
    assert!(buckets.iter().all(|b| b.label != "Sonnet"));
    assert!(buckets.iter().all(|b| b.label != "Opus"));
    assert!(buckets.iter().all(|b| b.label != "Daily Routines"));
}

/// A `weekly_scoped` window with no model display name is skipped rather than
/// fabricated into an empty-label row — the same "absent window must be
/// omitted, never fabricated" rule the legacy path follows.
#[test]
fn claude_oauth_limits_array_skips_unnamed_scoped_window() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "weekly_scoped", "group": "weekly", "percent": 40,
              "severity": "normal", "resets_at": "2026-07-03T06:59:59Z",
              "scope": { "model": { "id": null, "display_name": null } },
              "is_active": true }
        ]
    }))
    .expect("valid limits response");
    let buckets = usage.into_buckets(1_781_300_000);
    assert!(buckets.is_empty(), "unnamed scoped window must be omitted");
}

/// A non-empty `limits` array must not erase legacy windows that are still the
/// only usable source for a semantic slot. Current Claude responses can mix the
/// new carrier with older fields; unknown/partial limits backfill from legacy
/// windows, while duplicate Session/Weekly windows are not emitted twice.
#[test]
fn claude_oauth_limits_array_backfills_missing_legacy_windows() {
    let reset_at = "2026-07-03T06:59:59Z";
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 22.0, "resets_at": "2026-07-03T03:19:59Z" },
        "seven_day": { "utilization": 44.0, "resets_at": reset_at },
        "seven_day_sonnet": { "utilization": 55.0, "resets_at": reset_at },
        "limits": [
            { "kind": "session", "group": "session", "percent": 7,
              "severity": "normal", "resets_at": "2026-07-03T03:19:59Z",
              "scope": null },
            { "kind": "future_shape", "group": "weekly", "percent": 99,
              "severity": "normal", "resets_at": reset_at, "scope": null }
        ]
    }))
    .expect("mixed limits/legacy response");

    let buckets = usage.into_buckets(1_781_300_000);

    let session_buckets = buckets
        .iter()
        .filter(|b| b.status_slot == Some(StatusSlot::Session))
        .count();
    assert_eq!(session_buckets, 1, "Session duplicate must be skipped");
    let weekly = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Weekly))
        .expect("legacy Weekly backfill");
    assert_eq!(weekly.label, "Weekly");
    assert_eq!(weekly.remaining_percent, Some(56));
    let sonnet = buckets
        .iter()
        .find(|b| b.label == "Sonnet")
        .expect("legacy Sonnet backfill");
    assert_eq!(sonnet.remaining_percent, Some(45));
}

/// `limits.percent` is an external API boundary: accept string/float/over-cap
/// values instead of narrowing serde to `u8` before the existing percent helpers
/// can normalize and render them.
#[test]
fn claude_oauth_limits_percent_accepts_string_float_and_over_cap() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": "35.4",
              "severity": "normal", "resets_at": null, "scope": null },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 150.0,
              "severity": "danger", "resets_at": null,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("lenient percent response");

    let buckets = usage.into_buckets(1_781_300_000);
    let session = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Session))
        .expect("session bucket");
    assert_eq!(session.used_label.as_deref(), Some("35% used"));
    assert_eq!(session.remaining_percent, Some(65));
    let fable = buckets
        .iter()
        .find(|b| b.label == "Fable")
        .expect("Fable bucket");
    assert_eq!(fable.used_label.as_deref(), Some("150% used"));
    assert_eq!(fable.remaining_percent, Some(0));
}

/// The unified model: a legacy `seven_day_sonnet` window and a `limits`
/// `weekly_scoped` window carrying the same usage/resets produce the same
/// bucket (modulo label). This is the design invariant — Fable is not a
/// separate code path, it is the same path as a legacy model window, so a
/// regression that re-introduces parallel builders would fail here.
#[test]
fn claude_legacy_and_limits_sources_share_one_builder() {
    let reset_at = "2026-07-03T06:59:59Z";
    let now = 1_781_300_000;
    let legacy: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        // `utilization` is percent-form here (35.0 > 1.0), matching the limits
        // `percent` field so both resolve to 35% used through the same helpers.
        "seven_day_sonnet": { "utilization": 35.0, "resets_at": reset_at }
    }))
    .expect("legacy response");
    let limits: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "weekly_scoped", "group": "weekly", "percent": 35,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } }, "is_active": true }
        ]
    }))
    .expect("limits response");

    let sonnet = legacy
        .into_buckets(now)
        .into_iter()
        .find(|b| b.label == "Sonnet")
        .expect("legacy Sonnet bucket");
    let fable = limits
        .into_buckets(now)
        .into_iter()
        .find(|b| b.label == "Fable")
        .expect("limits Fable bucket");

    // Same builder ⇒ identical meter, pace, reset, and severity. Only the label
    // (the model the window is scoped to) differs.
    assert_eq!(sonnet.used_label, fable.used_label);
    assert_eq!(sonnet.remaining_percent, fable.remaining_percent);
    assert_eq!(sonnet.reset_label, fable.reset_label);
    assert_eq!(sonnet.resets_at, fable.resets_at);
    assert_eq!(sonnet.pace_label, fable.pace_label);
    assert_eq!(sonnet.severity, fable.severity);
    assert_eq!(sonnet.status_slot, fable.status_slot);
}

/// A single response can carry several model-scoped weekly windows at once
/// (Sonnet, Opus, Fable, …). Each `weekly_scoped` entry surfaces as its own
/// non-headline bucket, so "all models as before plus Fable" renders together
/// — the whole point of the generic handler (no per-model code).
#[test]
fn claude_limits_array_surfaces_every_scoped_model_together() {
    let reset_at = "2026-07-03T06:59:59Z";
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": 46,
              "severity": "normal", "resets_at": "2026-07-03T03:20:00Z", "scope": null },
            { "kind": "weekly_all", "group": "weekly", "percent": 36,
              "severity": "normal", "resets_at": reset_at, "scope": null },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 12,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Sonnet" } } },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 8,
              "severity": "normal", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Opus" } } },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 43,
              "severity": "warn", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("multi-model limits response");

    let buckets = usage.into_buckets(1_781_300_000);
    let labels: Vec<&str> = buckets.iter().map(|b| b.label.as_str()).collect();

    // Headline windows bind to their slots; every model-scoped window renders
    // as its own labelled, non-headline row.
    assert!(labels.contains(&"Session"));
    assert!(labels.contains(&"All models"));
    assert!(labels.contains(&"Sonnet"));
    assert!(labels.contains(&"Opus"));
    assert!(labels.contains(&"Fable"));
    for label in ["Sonnet", "Opus", "Fable"] {
        let b = buckets
            .iter()
            .find(|b| b.label == label)
            .unwrap_or_else(|| panic!("{label} bucket"));
        assert_eq!(b.status_slot, None, "{label} must be non-headline");
        assert!(b.remaining_percent.is_some(), "{label} must carry a meter");
    }
    // Fable carries its own (warn) severity for meter color, independent of the
    // other scoped windows.
    let fable = buckets.iter().find(|b| b.label == "Fable").expect("Fable");
    assert_eq!(fable.severity, UsageSeverity::Warn);
    assert_eq!(fable.remaining_percent, Some(57));
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

/// Bug 8: the compact headline drops every zero-value segment — a `0%` window
/// and `$0` spend — while the dialog (not this fn) still shows them.
#[test]
fn status_bar_headline_drops_zero_window_and_zero_spend() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 0.0, "resets_at": "2026-06-28T16:40:00Z" },
        "seven_day": { "utilization": 1.0, "resets_at": "2026-07-03T07:00:00Z" },
        "spend": {
            "used": { "amount_minor": 0, "currency": "USD", "exponent": 2 },
            "limit": { "amount_minor": 30000, "currency": "USD", "exponent": 2 },
            "percent": 0,
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");
    let buckets = usage.into_buckets(1_781_185_560);
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Claude, &buckets).as_deref(),
        Some("Session 100%"),
        "Weekly 0% and $0 spent must be omitted from the status bar"
    );
}

/// Class III-D: the per-account refresh lock is exclusive — a second acquirer of
/// the same account is told it is Held while the first holds it, and can acquire
/// once the first is released. (flock conflicts across distinct open file
/// descriptions even in-process, mirroring the cross-container behavior.)
#[test]
fn account_refresh_lock_is_exclusive_and_releases() {
    let dir = tempfile::tempdir().expect("tempdir");
    let key = "Anthropic#deadbeef";
    let first = acquire_account_refresh_lock_in(dir.path(), key);
    assert!(
        matches!(first, RefreshLockOutcome::Acquired(_)),
        "first acquirer wins the lock"
    );
    assert!(
        matches!(
            acquire_account_refresh_lock_in(dir.path(), key),
            RefreshLockOutcome::Held
        ),
        "second acquirer of the held lock is told it is Held"
    );
    // A different account is independent.
    assert!(matches!(
        acquire_account_refresh_lock_in(dir.path(), "OpenAI#feedface"),
        RefreshLockOutcome::Acquired(_)
    ));
    drop(first);
    assert!(
        matches!(
            acquire_account_refresh_lock_in(dir.path(), key),
            RefreshLockOutcome::Acquired(_)
        ),
        "lock is re-acquirable after release"
    );
}

/// Class III-C: hydrating a shared snapshot keeps its numbers but marks the view
/// and its buckets Stale (last-known, not freshly fetched by this instance).
#[test]
fn stale_shared_view_downgrades_status_keeps_numbers() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 15.0, "resets_at": "2026-06-28T16:40:00Z" }
    }))
    .expect("valid Claude OAuth usage");
    let buckets = usage.into_buckets(1_781_185_560);
    let fresh = FocusedUsageView {
        status: UsageSnapshotStatus::Fresh,
        buckets,
        fetched_at_epoch: 1_781_185_560,
        ..FocusedUsageView::unavailable("seed", 1_781_185_560)
    };
    let stale = stale_shared_view(fresh, 1_781_185_860);
    assert_eq!(stale.status, UsageSnapshotStatus::Stale);
    assert!(
        stale
            .buckets
            .iter()
            .all(|b| b.status == UsageSnapshotStatus::Stale),
        "every bucket downgraded to Stale"
    );
    let session = stale
        .buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Session))
        .expect("session bucket");
    assert_eq!(session.remaining_percent, Some(85), "numbers preserved");
}

/// Class III: a surface with no resolvable OAuth identity falls back to the
/// provider-surface key (preserving prior single-account behavior), so the
/// account-keying never breaks key-based providers.
#[test]
fn shared_account_key_falls_back_to_provider_for_unsupported() {
    let target = UsageRefreshTarget {
        agent: "totally-unknown-agent".to_owned(),
        provider: Some("NoSuchProvider".to_owned()),
    };
    assert_eq!(target.shared_account_key(), target.cache_key());
}

/// Bug 11: an over-cap window (>100% utilization) keeps its true used figure in
/// the label instead of being clamped to `100% used`; `remaining` stays 0.
#[test]
fn over_cap_window_surfaces_true_used_percent() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "seven_day": { "utilization": 150.0, "resets_at": "2026-07-03T07:00:00Z" }
    }))
    .expect("valid Claude OAuth usage");
    let buckets = usage.into_buckets(1_781_185_560);
    let weekly = buckets
        .iter()
        .find(|b| b.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly bucket");
    assert_eq!(
        weekly.remaining_percent,
        Some(0),
        "nothing left when over cap"
    );
    assert_eq!(weekly.used_label.as_deref(), Some("150% used"));
}

/// Bug 5: the overview headline bucket is the tightest *windowed* bucket that
/// carries a reset — never the reset-less spend bucket, even when spend has the
/// lowest remaining (so the overview row keeps its reset column).
#[test]
fn most_constrained_skips_reset_less_spend_for_windowed_reset_bucket() {
    let usage: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": { "utilization": 11.0, "resets_at": "2026-06-28T16:40:00Z" },
        "seven_day": { "utilization": 27.0, "resets_at": "2026-07-03T07:00:00Z" },
        "spend": {
            "used": { "amount_minor": 9000, "currency": "USD", "exponent": 2 },
            "limit": { "amount_minor": 30000, "currency": "USD", "exponent": 2 },
            "percent": 30,
            "enabled": true
        }
    }))
    .expect("valid Claude OAuth usage");
    // Spend = 70% left (lowest), but reset-less; Weekly = 73% left with a reset.
    let buckets = usage.into_buckets(1_781_185_560);
    let chosen = most_constrained_fresh_bucket(&buckets).expect("a windowed bucket");
    assert_eq!(chosen.status_slot, Some(StatusSlot::Weekly));
    assert!(
        chosen.resets_at.is_some(),
        "chosen bucket must carry a reset"
    );
}

/// The compact Overview/status row names the bottleneck model when a
/// model-scoped window (Fable) is the most-constrained, but stays bare for a
/// headline window (Session/Weekly) — the slot already implies those and the
/// status bar carries them. So an operator watching the compact surface learns
/// *which* model is the limit, not just the % left.
#[test]
fn usage_tab_status_label_names_scoped_model_when_it_is_most_constrained() {
    let reset_at = "2026-07-03T06:59:59Z";
    // Fable (10% left) is tighter than Session (50% left) and Weekly (60%).
    let fable_wins: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": 50,
              "severity": "normal", "resets_at": "2026-07-03T03:20:00Z", "scope": null },
            { "kind": "weekly_all", "group": "weekly", "percent": 40,
              "severity": "normal", "resets_at": reset_at, "scope": null },
            { "kind": "weekly_scoped", "group": "weekly", "percent": 90,
              "severity": "danger", "resets_at": reset_at,
              "scope": { "model": { "display_name": "Fable" } } }
        ]
    }))
    .expect("limits response");
    let mut view = FocusedUsageView::unavailable("x", 1_781_300_000);
    view.status = UsageSnapshotStatus::Fresh;
    view.buckets = fable_wins.into_buckets(1_781_300_000);
    let label = usage_tab_status_label(&view);
    assert!(
        label.starts_with("Fable 10% left"),
        "scoped bottleneck must be named: got {label:?}"
    );

    // When a headline window (Session) is tightest, no model name is prepended.
    let session_wins: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "limits": [
            { "kind": "session", "group": "session", "percent": 95,
              "severity": "warn", "resets_at": "2026-07-03T03:20:00Z", "scope": null },
            { "kind": "weekly_all", "group": "weekly", "percent": 10,
              "severity": "normal", "resets_at": reset_at, "scope": null }
        ]
    }))
    .expect("limits response");
    view.buckets = session_wins.into_buckets(1_781_300_000);
    let label = usage_tab_status_label(&view);
    assert!(
        label.starts_with("5% left"),
        "headline bottleneck must stay bare: got {label:?}"
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

fn codex_minimal_limits_value() -> serde_json::Value {
    serde_json::json!({
        "rateLimits": {
            "primary": {
                "usedPercent": 25.0,
                "windowDurationMins": 300,
                "resetsAt": 1_781_189_520_i64
            }
        }
    })
}

#[test]
fn codex_rpc_account_api_key_tag_yields_origin_label_and_rate_limits() {
    let usage = codex::decode_codex_rpc_usage(
        codex_minimal_limits_value(),
        Some(serde_json::json!({ "account": { "type": "apiKey" } })),
    )
    .expect("Codex RPC decode");
    assert_eq!(usage.account_label.as_deref(), Some("Codex API key"));
    let buckets = usage.response.buckets(1_781_185_560);
    assert!(buckets.iter().any(|bucket| bucket.label == "Session"));
}

#[test]
fn codex_rpc_account_amazon_bedrock_tag_decodes_without_label() {
    let account: CodexRpcAccountResponse = serde_json::from_value(serde_json::json!({
        "account": { "type": "amazonBedrock", "usesCodexManagedCredentials": true }
    }))
    .expect("Codex Bedrock account decodes");
    let limits: CodexRpcRateLimitsResponse =
        serde_json::from_value(codex_minimal_limits_value()).expect("limits");
    let usage = CodexRpcUsage::from_rpc(limits, Some(account));
    assert_eq!(usage.account_label, None);
}

#[test]
fn codex_rpc_account_decode_failure_degrades_to_no_label() {
    let usage = codex::decode_codex_rpc_usage(
        codex_minimal_limits_value(),
        Some(serde_json::json!({ "account": { "type": "someFutureTag" } })),
    )
    .expect("unknown account tag still yields usage");
    assert_eq!(usage.account_label, None);
    let buckets = usage.response.buckets(1_781_185_560);
    assert!(buckets.iter().any(|bucket| bucket.label == "Session"));
}

#[test]
fn managed_cli_launch_gate_cools_down_after_launch_failure() {
    let mut gate = ManagedCliLaunchGate::default();
    gate.can_launch("probe", Instant::now()).unwrap();

    gate.record_launch_failure("blocked".to_owned());

    let error = gate
        .can_launch("probe", Instant::now())
        .expect_err("cooldown should block launch");
    assert!(error.contains("cooldown active"));
    assert!(error.contains("blocked"));

    gate.record_success();
    gate.can_launch("probe", Instant::now()).unwrap();
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

/// The CLI prints per-model weekly lines as `Current week (<model>): …` (Fable
/// today, future codenames tomorrow). The parser captures each generically so
/// a new model prints without a per-model edit. Mirrors the live 2026-07-03
/// `claude -p /usage` output, where Sonnet was replaced by Fable.
#[test]
fn claude_cli_usage_output_maps_scoped_weekly_fable() {
    let usage = parse_claude_usage_output(
        "You are currently using your subscription to power your Claude Code usage\n\
             \n\
             Current session: 9% used · resets Jul 3 at 10:19am (Asia/Saigon)\n\
             Current week (all models): 28% used · resets Jul 3 at 2pm (Asia/Saigon)\n\
             Current week (Fable): 35% used · resets Jul 3 at 1:59pm (Asia/Saigon)\n",
    )
    .expect("usage output");

    // The model-scoped line lands in `scoped_weekly` (not `sonnet_used`).
    assert_eq!(usage.scoped_weekly.len(), 1);
    assert_eq!(usage.scoped_weekly[0].0, "Fable");
    assert!((usage.scoped_weekly[0].1 - 35.0).abs() < f64::EPSILON);

    let buckets = usage.buckets();
    let fable = buckets
        .iter()
        .find(|b| b.label == "Fable")
        .expect("Fable CLI bucket");
    assert_eq!(fable.remaining_percent, Some(65));
    assert_eq!(fable.status_slot, None);

    // Headline still binds to the slot from the explicit (all models) line.
    assert_eq!(
        status_bar_label(
            UsageSurface::Claude,
            "",
            UsageSnapshotStatus::Fresh,
            &buckets
        ),
        "Session 91% · Weekly 72%"
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
fn grok_billing_config_maps_current_fallback_and_bounds() {
    let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "subscription_tier": "SuperGrok",
        "on_demand_enabled": true,
        "config": {
            "monthlyLimit": { "val": 5000 },
            "used": { "val": 1800 },
            "billingPeriodStart": "2026-06-01T00:00:00Z",
            "billingPeriodEnd": "2026-07-01T00:00:00Z",
            "prepaidBalance": { "val": 2500 },
            "onDemandCap": { "val": 4000 },
            "onDemandUsed": { "val": 300 }
        }
    }))
    .expect("valid current Grok billing response");

    assert_eq!(usage.plan_label().as_deref(), Some("SuperGrok"));
    let buckets = usage.buckets(1_780_315_200);

    // One headline (Weekly slot), detail rows stay untagged.
    assert_eq!(buckets[0].label, "Monthly");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[0].remaining_percent, Some(64));
    assert!(
        buckets[1..]
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );

    let credits = buckets
        .iter()
        .find(|bucket| bucket.label == "Extra usage credits")
        .expect("prepaid balance bound");
    assert_eq!(credits.limit_label.as_deref(), Some("$25"));
    assert!(credits.limit_money.is_some());
    assert_eq!(credits.used_label, None);

    let on_demand = buckets
        .iter()
        .find(|bucket| bucket.label == "On-demand usage")
        .expect("on-demand bound");
    assert_eq!(on_demand.used_label.as_deref(), Some("$3"));
    assert_eq!(on_demand.limit_label.as_deref(), Some("$40"));
    assert!(on_demand.limit_money.is_some());
}

#[test]
fn grok_billing_config_preferred_percent_path_has_pace() {
    let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "creditUsagePercent": 43.0,
            "currentPeriod": {
                "type": "USAGE_PERIOD_TYPE_WEEKLY",
                "start": "2026-06-01T00:00:00Z",
                "end": "2026-06-08T00:00:00Z"
            }
        }
    }))
    .expect("valid preferred config");
    let now = parse_iso_epoch("2026-06-04T00:00:00Z").expect("now");
    let buckets = usage.buckets(now);
    assert_eq!(buckets[0].label, "Weekly");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Weekly));
    assert_eq!(buckets[0].remaining_percent, Some(57));
    assert!(buckets[0].pace_label.is_some());
}

#[test]
fn grok_plan_label_from_server_tier_only() {
    let free: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "config": {},
        "subscription_tier": "  "
    }))
    .expect("blank tier");
    assert_eq!(free.plan_label(), None);
    // Web path never guesses a plan (no auth heuristic).
    let web = GrokBillingSnapshot::Web(GrokWebBillingSnapshot {
        used_percent: 40.0,
        reset_at_epoch: Some(1_780_315_200),
    });
    assert_eq!(web.plan_label(), None);
}

#[test]
fn grok_on_demand_requires_positive_cap() {
    let usage: GrokBillingResponse = serde_json::from_value(serde_json::json!({
        "on_demand_enabled": true,
        "config": { "onDemandUsed": { "val": 500 } }
    }))
    .expect("no cap config");
    let buckets = usage.buckets(1_780_315_200);
    // Used without a positive provider cap is unbounded spend, not a quota bound.
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.label != "On-demand usage")
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
        "config": {
            "monthlyLimit": { "val": 5000 },
            "used": { "val": 1000 },
            "billingPeriodStart": "2026-06-01T00:00:00Z",
            "billingPeriodEnd": "2026-07-01T00:00:00Z"
        }
    }))
    .expect("valid current Grok billing response");

    let view = grok_snapshot_from_rpc_result(
        "grok",
        1_780_315_200,
        missing,
        false,
        false,
        false,
        Ok(GrokBillingSnapshot::Rpc(Box::new(billing))),
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
    // of the window still remaining -> 30 points of deficit, and the linear
    // projection runs out before the reset (Variant A composite).
    let deficit = quota_pace_label(Some(60), Some(900), Some(1_000), 0).expect("pace label");
    assert_eq!(deficit, "30% in deficit · Runs out in 2m");

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
        format!(
            "Resets in 1h 26m ({})",
            format::local_timestamp_label(same_day)
        )
    );
    let tomorrow = parse_iso_epoch("2026-06-12T04:18:00Z").expect("tomorrow");
    assert_eq!(
        reset_label(tomorrow, now),
        format!(
            "Resets in 14h 32m ({})",
            format::local_timestamp_label(tomorrow)
        )
    );
    let future = parse_iso_epoch("2026-07-01T16:31:00Z").expect("future");
    assert_eq!(
        reset_label(future, now),
        format!(
            "Resets in 20d 2h ({})",
            format::local_timestamp_label(future)
        )
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

const AMP_DAILY_FIXTURE: &str = "Signed in as user@example.com (example)\n\
     Amp Free: 61% remaining today (resets daily)\n\
     Individual credits: $9.86 remaining\n\
     Workspace example: $5.33 remaining";

const AMP_TWO_WORKSPACE_FIXTURE: &str = "Amp Free: 61% remaining today (resets daily)\n\
     Individual credits: $9.86 remaining\n\
     Workspace alpha: $5.33 remaining\n\
     Workspace beta: $2.25 remaining";

#[test]
fn amp_daily_display_text_maps_daily_slot_and_reset_description() {
    let api = AmpUsage::from_api_value(serde_json::json!({
        "result": { "displayText": AMP_DAILY_FIXTURE }
    }))
    .expect("Amp API daily usage");
    let cli = parse_amp_usage_output(AMP_DAILY_FIXTURE).expect("Amp CLI daily usage");

    // API and CLI delegate to one parser: identical parsed fields.
    assert_eq!(api.account_label.as_deref(), Some("user@example.com"));
    assert_eq!(api.account_label, cli.account_label);
    assert_eq!(api.daily_remaining_percent, Some(61));
    assert_eq!(api.daily_remaining_percent, cli.daily_remaining_percent);
    assert_eq!(api.individual_credits, cli.individual_credits);
    assert_eq!(api.workspace_balances, cli.workspace_balances);

    let buckets = api.buckets();
    assert_eq!(buckets[0].label, "Amp Free");
    assert_eq!(buckets[0].status_slot, Some(StatusSlot::Daily));
    assert_eq!(buckets[0].remaining_percent, Some(61));
    assert_eq!(buckets[0].reset_label.as_deref(), Some("Resets daily"));
    assert_eq!(buckets[0].resets_at, None);
}

#[test]
fn amp_daily_percentage_clamps_to_protocol_range() {
    let high = parse_amp_usage_output("Amp Free: 140% remaining today (resets daily)")
        .expect("high daily");
    assert_eq!(high.daily_remaining_percent, Some(100));
    let low =
        parse_amp_usage_output("Amp Free: -5% remaining today (resets daily)").expect("low daily");
    assert_eq!(low.daily_remaining_percent, Some(0));
    // A malformed/non-finite percent yields no Daily bucket.
    assert!(parse_amp_usage_output("Amp Free: abc% remaining today (resets daily)").is_none());
}

#[test]
fn amp_daily_parser_preserves_workspace_balances_in_order() {
    let usage = parse_amp_usage_output(AMP_TWO_WORKSPACE_FIXTURE).expect("two workspace");
    assert_eq!(usage.individual_credits, Some(9.86));
    assert_eq!(
        usage.workspace_balances,
        vec![
            AmpWorkspaceBalance {
                name: "alpha".to_owned(),
                remaining: 5.33,
            },
            AmpWorkspaceBalance {
                name: "beta".to_owned(),
                remaining: 2.25,
            },
        ]
    );
    let buckets = usage.buckets();
    let labels: Vec<_> = buckets.iter().map(|bucket| bucket.label.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Amp Free",
            "Individual credits",
            "Workspace alpha",
            "Workspace beta"
        ]
    );
    // Only the Amp Free bucket carries a status slot.
    assert!(
        buckets[1..]
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );
}

#[test]
fn amp_paid_only_balances_do_not_infer_daily_or_plan() {
    let usage = parse_amp_usage_output(
        "Signed in as user@example.com (example)\n\
         Individual credits: $9.86 remaining\n\
         Workspace example: $5.33 remaining",
    )
    .expect("paid-only usage");
    assert_eq!(usage.plan_label(), None);
    let buckets = usage.buckets();
    assert!(
        buckets
            .iter()
            .all(|bucket| bucket.status_slot != Some(StatusSlot::Daily))
    );
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Amp, &buckets),
        None
    );

    // The Fresh, Authoritative view preserves provenance and has no plan label.
    for source in [UsageSource::ProviderApi, UsageSource::Cli] {
        let view = amp_view_from_usage(
            AmpSuccessContext {
                agent: "amp",
                credential_origin: Some("API key · env AMP_API_KEY".to_owned()),
                source,
            },
            usage.clone(),
            1_781_185_560,
        );
        assert_eq!(view.status, UsageSnapshotStatus::Fresh);
        assert_eq!(view.confidence, UsageConfidence::Authoritative);
        assert_eq!(view.source, source);
        assert_eq!(
            view.account.credential_origin.as_deref(),
            Some("API key · env AMP_API_KEY")
        );
        assert_eq!(view.account.plan_label, None);
        assert!(
            view.buckets
                .iter()
                .any(|bucket| bucket.label == "Individual credits")
        );
    }

    // A Daily bucket beside credits yields the daily headline, never a credit amount.
    let mut with_daily = usage.clone();
    with_daily.daily_remaining_percent = Some(61);
    assert_eq!(
        status_bar_headline_for_surface(UsageSurface::Amp, &with_daily.buckets()).as_deref(),
        Some("Free 61%")
    );
}

#[test]
fn amp_legacy_hourly_display_text_is_rejected() {
    // The retired hourly-dollar line alone parses to nothing.
    assert!(
        parse_amp_usage_output("Amp Free: $2.42/$10 remaining (replenishes +$0.42/hour)").is_none()
    );
    // Paired with current credit rows it contributes no Amp Free bucket.
    let usage = parse_amp_usage_output(
        "Amp Free: $2.42/$10 remaining (replenishes +$0.42/hour)\n\
         Individual credits: $0.33 remaining",
    )
    .expect("credit rows");
    assert_eq!(usage.daily_remaining_percent, None);
    assert!(
        usage
            .buckets()
            .iter()
            .all(|bucket| bucket.status_slot != Some(StatusSlot::Daily))
    );
}

#[test]
fn cli_output_collector_treats_reaped_child_as_success() {
    let output = format::collect_cli_output(
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

#[cfg(unix)]
#[test]
fn usage_cli_owner_exports_outcomes_without_process_material() {
    use std::io::Write as _;
    use std::os::unix::fs::PermissionsExt as _;

    let directory = tempfile::tempdir().unwrap();
    let executable = directory.path().join("claude");
    let mut file = fs::File::create(&executable).unwrap();
    writeln!(file, "#!/bin/sh\nexec sh \"$@\"").unwrap();
    let mut permissions = file.metadata().unwrap().permissions();
    permissions.set_mode(0o700);
    file.set_permissions(permissions).unwrap();
    drop(file);
    let command = executable.to_string_lossy();

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    let _subscriber = tracing::subscriber::set_default(subscriber);

    // Success/error paths must outlive heavy parallel nextest load; 1s races
    // under full `ci --fast` when the host is saturated (poll loop is 50ms).
    let settle = Duration::from_secs(10);
    run_cli_with_timeout_full(&command, &["-c", "printf usage-secret-output"], settle).unwrap();
    run_cli_with_timeout_full(
        &command,
        &["-c", "printf usage-secret-stderr >&2; exit 17"],
        settle,
    )
    .unwrap();
    let _timeout =
        run_cli_with_timeout_full(&command, &["-c", "sleep 1"], Duration::from_millis(5))
            .unwrap_err();
    let _spawn = run_cli_with_timeout_full(
        "/usage-secret/missing/claude",
        &["usage-secret-argument"],
        settle,
    )
    .unwrap_err();

    export.force_flush();
    assert_eq!(export.finished_spans().len(), 4);
    assert_eq!(export.error_span_count(), 3);
    for expected in [
        "claude",
        "process_exit_nonzero",
        "process_spawn_error",
        "timeout",
    ] {
        assert!(export.contains_span_text(expected), "missing {expected}");
    }
    for prohibited in [
        command.as_ref(),
        "usage-secret-output",
        "usage-secret-stderr",
        "/usage-secret/missing/claude",
        "usage-secret-argument",
    ] {
        assert!(!export.contains_span_text(prohibited));
    }
}

#[test]
fn usage_cli_output_capture_is_bounded() {
    let oversized = vec![b'x'; format::PROCESS_OUTPUT_MAX + 1];
    assert_eq!(
        format::read_process_pipe(std::io::Cursor::new(oversized)).unwrap_err(),
        "process output exceeded limit"
    );
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
fn zai_plan_label_falls_back_to_level() {
    let tokens_limit = serde_json::json!({
        "type": "TOKENS_LIMIT",
        "unit": 5,
        "number": 300,
        "usage": 1000,
        "currentValue": 250,
        "remaining": 750,
        "percentage": 25,
        "nextResetTime": 1_781_189_520_000_i64
    });
    // `level` present, no `planName`: the one plan field observed in the wild.
    let level_only: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "data": { "level": "pro", "limits": [tokens_limit.clone()] }
    }))
    .expect("level-only quota");
    assert_eq!(level_only.plan_name().as_deref(), Some("pro"));

    // Both present parses without a duplicate-field error; explicit name wins.
    let both: ZaiQuotaResponse = serde_json::from_value(serde_json::json!({
        "code": 200,
        "success": true,
        "data": { "planName": "Coding Pro", "level": "pro", "limits": [tokens_limit] }
    }))
    .expect("planName + level quota");
    assert_eq!(both.plan_name().as_deref(), Some("Coding Pro"));
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
fn minimax_remains_urls_include_documented_host() {
    assert_eq!(
        resolve_minimax_remains_urls_from(None, None),
        vec![
            "https://api.minimax.io/v1/token_plan/remains",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
            "https://api.minimaxi.com/v1/token_plan/remains",
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
            "https://www.minimax.io/v1/token_plan/remains",
        ]
    );
}

#[test]
fn minimax_fanout_reaches_documented_host_after_four_failures() {
    let mut attempted = Vec::new();
    let result = first_minimax_usage(resolve_minimax_remains_urls_from(None, None), |url| {
        attempted.push(url.to_owned());
        if url == "https://www.minimax.io/v1/token_plan/remains" {
            Ok("documented")
        } else {
            Err(format!("HTTP 500 for {url}"))
        }
    });
    assert_eq!(result, Ok("documented"));
    assert_eq!(
        attempted,
        vec![
            "https://api.minimax.io/v1/token_plan/remains",
            "https://api.minimax.io/v1/api/openplatform/coding_plan/remains",
            "https://api.minimaxi.com/v1/token_plan/remains",
            "https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains",
            "https://www.minimax.io/v1/token_plan/remains",
        ]
    );
}

#[test]
fn minimax_empty_fanout_preserves_unavailable_error() {
    let mut calls = 0;
    let result: Result<&str, String> = first_minimax_usage(Vec::new(), |_url| {
        calls += 1;
        Ok("unreachable")
    });
    assert_eq!(calls, 0);
    assert_eq!(result, Err("MiniMax usage endpoint unavailable".to_owned()));
}

#[test]
fn minimax_operation_path_matches_candidate_path() {
    assert_eq!(
        minimax_operation_path("https://api.minimax.io/v1/token_plan/remains"),
        "/v1/token_plan/remains"
    );
    assert_eq!(
        minimax_operation_path("https://www.minimax.io/v1/token_plan/remains"),
        "/v1/token_plan/remains"
    );
    assert_eq!(
        minimax_operation_path("https://api.minimax.io/v1/api/openplatform/coding_plan/remains"),
        "/v1/api/openplatform/coding_plan/remains"
    );
    assert_eq!(
        minimax_operation_path("https://api.minimaxi.com/v1/api/openplatform/coding_plan/remains"),
        "/v1/api/openplatform/coding_plan/remains"
    );
    // An arbitrary override never exposes its real path in telemetry.
    assert_eq!(
        minimax_operation_path("https://quota.example/custom/remains?tenant=secret"),
        "/custom"
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

#[test]
fn provider_boundary_exports_only_bounded_request_fields() {
    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let result = provider_request(
            jackin_telemetry::schema::enums::ProviderName::Openai,
            "GET",
            "/backend-api/wham/usage",
            || Ok::<_, String>("telemetry-private-response"),
        );
        assert_eq!(result.unwrap(), "telemetry-private-response");
    });
    export.force_flush();

    let spans = export
        .finished_spans()
        .into_iter()
        .filter(|span| span.name == jackin_telemetry::schema::spans::HTTP_CLIENT)
        .collect::<Vec<_>>();
    assert_eq!(spans.len(), 1);
    for prohibited in [
        "authorization",
        "account_id",
        "telemetry-private-response",
        "?private=query",
    ] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn conformance_wire_provider_boundary_exports_bounded_private_shapes() {
    let testbed = jackin_otlp_testbed::Testbed::start().expect("start OTLP testbed");
    jackin_diagnostics::init_wire_test_export(
        &testbed.endpoint(),
        jackin_diagnostics::ServiceIdentity::CAPSULE,
    )
    .expect("initialize wire test export");

    let success = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Openai,
        "GET",
        "/backend-api/wham/usage",
        || Ok::<_, String>("private-provider-response"),
    );
    assert_eq!(
        success.expect("provider request succeeds"),
        "private-provider-response"
    );
    let failure = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Anthropic,
        "POST",
        "/api/oauth/usage",
        || Err::<(), _>("private-token private-account ?private=query".to_owned()),
    );
    assert!(failure.is_err());
    jackin_diagnostics::flush_wire_test_export().expect("flush wire test export");

    let deadline = Instant::now() + Duration::from_secs(2);
    let spans = loop {
        let spans = testbed
            .spans()
            .into_iter()
            .filter(|span| span.name == "http.client")
            .collect::<Vec<_>>();
        if spans.len() == 2 {
            break spans;
        }
        assert!(
            Instant::now() < deadline,
            "provider HTTP wire spans did not arrive"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    let wire_text = format!("{spans:?}");
    for expected in [
        "openai",
        "anthropic",
        "GET",
        "POST",
        "/backend-api/wham/usage",
        "/api/oauth/usage",
        "success",
        "failure",
        "http_error",
    ] {
        assert!(
            wire_text.contains(expected),
            "missing {expected}: {wire_text}"
        );
    }
    let prohibited = [
        "private-provider-response",
        "private-token",
        "private-account",
        "?private=query",
    ];
    for value in prohibited {
        assert!(!wire_text.contains(value), "exported {value}");
    }
    assert_eq!(
        testbed.prohibited_value_violations(&prohibited),
        Vec::<String>::new()
    );
    assert_eq!(testbed.legacy_namespace_violations(), Vec::<String>::new());
    jackin_diagnostics::shutdown_capsule_tracing();
}

#[test]
fn managed_probe_boundaries_export_fixed_private_shapes() {
    use std::sync::mpsc;

    let (export, subscriber) = jackin_diagnostics::observability::test_capsule_layers(false);
    tracing::subscriber::with_default(subscriber, || {
        let codex = crate::process_telemetry::ChildOperation::begin("codex");
        codex.spawn_failed();
        let grok = crate::process_telemetry::ChildOperation::begin("/private/bin/grok");
        grok.io_failed();

        let (codex_tx, codex_rx) = mpsc::channel();
        codex_tx
            .send(
                serde_json::json!({
                    "id": 1,
                    "result": {"private_response": "codex-secret"}
                })
                .to_string(),
            )
            .unwrap();
        let mut codex_wire = Vec::new();
        codex_rpc_request(
            &mut codex_wire,
            &codex_rx,
            1,
            "account/rateLimits/read",
            serde_json::json!({"private_request": "codex-secret"}),
            Duration::from_secs(1),
        )
        .unwrap();
        codex_rpc_notification(&mut codex_wire, "initialized").unwrap();

        let (grok_tx, grok_rx) = mpsc::channel();
        grok_tx
            .send(
                serde_json::json!({
                    "id": 2,
                    "error": {"message": "grok-private-error"}
                })
                .to_string(),
            )
            .unwrap();
        let mut grok_wire = Vec::new();
        grok_rpc_request(
            &mut grok_wire,
            &grok_rx,
            2,
            "x.ai/billing",
            serde_json::json!({"private_request": "grok-secret"}),
            Duration::from_secs(1),
        )
        .unwrap_err();

        let (_timeout_tx, timeout_rx) = mpsc::channel();
        codex_rpc_request(
            &mut Vec::new(),
            &timeout_rx,
            3,
            "account/read",
            serde_json::json!({}),
            Duration::from_millis(1),
        )
        .unwrap_err();
    });
    export.force_flush();

    let spans = export.finished_spans();
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == jackin_telemetry::schema::spans::PROCESS_COMMAND)
            .count(),
        2
    );
    assert_eq!(
        spans
            .iter()
            .filter(|span| span.name == jackin_telemetry::schema::spans::RPC_CLIENT)
            .count(),
        4
    );
    for expected in [
        "codex",
        "grok",
        "codex.app-server",
        "grok.acp",
        "account/rateLimits/read",
        "account/read",
        "initialized",
        "x.ai/billing",
        "process_spawn_error",
        "io_error",
        "rpc_error",
        "timeout",
    ] {
        assert!(export.contains_span_text(expected), "missing {expected}");
    }
    for prohibited in [
        "/private/bin/grok",
        "private_request",
        "private_response",
        "codex-secret",
        "grok-secret",
        "grok-private-error",
    ] {
        assert!(!export.contains_span_text(prohibited));
        assert!(!export.contains_log_text(prohibited));
    }
}

// ===== Plan 002: Claude Keychain credential source =====

fn keychain_test_scope(is_default: bool) -> jackin_core::ClaudeKeychainScope {
    jackin_core::ClaudeKeychainScope {
        normalized_config_dir: PathBuf::from(if is_default {
            "/home/u/.claude"
        } else {
            "/home/u/.claude-work"
        }),
        service: if is_default {
            "Claude Code-credentials".to_owned()
        } else {
            "Claude Code-credentials-3342f2c7".to_owned()
        },
        is_default,
    }
}

const KEYCHAIN_PAYLOAD: &str = r#"{"claudeAiOauth":{"accessToken":"kc-token","subscriptionType":"max","refreshToken":"rt-1"}}"#;

fn empty_file_probe() -> ClaudeFileProbe {
    ClaudeFileProbe {
        credential: None,
        origin: None,
        account_email: None,
        organization_type: None,
    }
}

#[test]
fn classify_claude_keychain_status_maps_denial_and_absence() {
    assert!(matches!(
        classify_claude_keychain_status(-128),
        ClaudeKeychainRead::Denied
    ));
    assert!(matches!(
        classify_claude_keychain_status(-25293),
        ClaudeKeychainRead::Denied
    ));
    assert!(matches!(
        classify_claude_keychain_status(-25300),
        ClaudeKeychainRead::Missing
    ));
    assert!(matches!(
        classify_claude_keychain_status(-25308),
        ClaudeKeychainRead::Missing
    ));
    assert!(matches!(
        classify_claude_keychain_status(-1),
        ClaudeKeychainRead::Missing
    ));
}

#[test]
fn claude_keychain_credential_wins_over_file_paths() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_service| ClaudeKeychainRead::Payload {
            json: KEYCHAIN_PAYLOAD.to_owned(),
        },
        || ClaudeFileProbe {
            credential: claude_oauth_from_value(
                &serde_json::json!({"claudeAiOauth":{"accessToken":"file-token"}}),
            ),
            origin: Some("OAuth · file".to_owned()),
            account_email: Some("user@example.com".to_owned()),
            organization_type: Some("Max".to_owned()),
        },
        || Some("env-token".to_owned()),
    );
    match resolution {
        ClaudeWaveResolution::Resolved(resolved) => {
            assert_eq!(resolved.access_token, "kc-token");
            assert_eq!(
                resolved.credential_origin,
                "OAuth · macOS Keychain (Claude Code-credentials)"
            );
            assert!(!resolved.is_anonymous);
        }
        _ => panic!("expected Resolved"),
    }
    assert_eq!(state.read_count(), 1);
}

#[test]
fn claude_keychain_denial_short_circuits_before_file_or_env_read() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_service| ClaudeKeychainRead::Denied,
        || panic!("file probe must not run after denial"),
        || panic!("env reader must not run after denial"),
    );
    assert!(matches!(resolution, ClaudeWaveResolution::Denied));
    // Terminal for the service: a later wave whose reader panics still returns
    // Denied from the process-lifetime cache without re-prompting.
    let again = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_service| panic!("reader must not run after cached denial"),
        || panic!("no file probe"),
        || panic!("no env"),
    );
    assert!(matches!(again, ClaudeWaveResolution::Denied));
    assert_eq!(state.read_count(), 1);
    assert_eq!(claude_wave_policy(&again), ClaudeWavePolicy::LocalDenied);
}

#[test]
fn claude_keychain_missing_falls_back_to_file_then_env() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let with_file = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_| ClaudeKeychainRead::Missing,
        || ClaudeFileProbe {
            credential: claude_oauth_from_value(
                &serde_json::json!({"claudeAiOauth":{"accessToken":"file-token","refreshToken":"rt"}}),
            ),
            origin: Some("OAuth · file".to_owned()),
            account_email: None,
            organization_type: None,
        },
        || None,
    );
    match with_file {
        ClaudeWaveResolution::Resolved(r) => assert_eq!(r.access_token, "file-token"),
        _ => panic!("file fallback"),
    }
    let state2 = ClaudeKeychainState::default();
    let with_env = resolve_claude_refresh_wave_with(
        &scope,
        &state2,
        |_| ClaudeKeychainRead::Missing,
        empty_file_probe,
        || Some("env-token".to_owned()),
    );
    match &with_env {
        ClaudeWaveResolution::Resolved(r) => {
            assert_eq!(r.access_token, "env-token");
            assert!(r.is_anonymous);
        }
        _ => panic!("env fallback"),
    }
    assert_eq!(
        claude_wave_policy(&with_env),
        ClaudeWavePolicy::LocalAnonymous
    );
}

#[test]
fn claude_keychain_missing_with_no_credential_is_local_missing() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_| ClaudeKeychainRead::Missing,
        empty_file_probe,
        || None,
    );
    assert!(matches!(resolution, ClaudeWaveResolution::Missing));
    assert_eq!(
        claude_wave_policy(&resolution),
        ClaudeWavePolicy::LocalMissing
    );
}

#[test]
fn claude_keychain_metadata_makes_resolution_shared() {
    let scope = keychain_test_scope(true);
    let state = ClaudeKeychainState::default();
    let resolution = resolve_claude_refresh_wave_with(
        &scope,
        &state,
        |_| ClaudeKeychainRead::Payload {
            json: r#"{"claudeAiOauth":{"accessToken":"kc"}}"#.to_owned(),
        },
        || ClaudeFileProbe {
            credential: None,
            origin: None,
            account_email: Some("id@example.com".to_owned()),
            organization_type: Some("Max".to_owned()),
        },
        || None,
    );
    match &resolution {
        ClaudeWaveResolution::Resolved(r) => {
            assert!(!r.is_anonymous);
            assert_eq!(r.account_email.as_deref(), Some("id@example.com"));
        }
        _ => panic!("resolved"),
    }
    assert_eq!(claude_wave_policy(&resolution), ClaudeWavePolicy::Shared);
}

#[test]
fn claude_denied_view_has_no_quota_and_exact_error() {
    let view = claude_view_from_wave(
        "claude",
        Some("Anthropic / Claude"),
        1_781_185_560,
        ClaudeWaveResolution::Denied,
    );
    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert!(view.buckets.is_empty());
    assert!(view.account.account_label.is_empty());
    assert_eq!(view.account.plan_label, None);
    assert_eq!(view.account.credential_origin, None);
    assert_eq!(
        view.last_error.as_deref(),
        Some("Claude Keychain access denied")
    );
}

// ===== Plan 005 Step 1: shared bucket-presentation formatter =====

fn presentation_bucket(
    label: &str,
    remaining: Option<u8>,
    slot: Option<StatusSlot>,
    status: UsageSnapshotStatus,
) -> QuotaBucketView {
    QuotaBucketView {
        label: label.to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: remaining,
        reset_label: None,
        resets_at: None,
        status_slot: slot,
        pace_label: None,
        status,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

#[test]
fn usage_bucket_presentation_orders_normal_segments() {
    let mut bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.pace_label = Some("13% in deficit · Runs out in 2d".to_owned());
    bucket.reset_label = Some("Resets in 4d".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec![
            "57% left",
            "13% in deficit",
            "Runs out in 2d",
            "Resets in 4d"
        ]
    );
    assert_eq!(presentation.remaining_label.as_deref(), Some("57% left"));
    assert_eq!(presentation.meter_percent, Some(57));
    assert_eq!(
        presentation.display_label,
        "57% left · 13% in deficit · Runs out in 2d · Resets in 4d"
    );
}

#[test]
fn usage_bucket_presentation_flattens_runout_composite() {
    let mut bucket = presentation_bucket(
        "Weekly",
        Some(40),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.pace_label = Some("On pace · Runs out in 5d".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec!["40% left", "On pace", "Runs out in 5d"]
    );
}

#[test]
fn usage_bucket_presentation_orders_spend_cap() {
    let mut bucket = presentation_bucket(
        "Extra usage",
        Some(70),
        Some(StatusSlot::Spend),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("SGD 78.49".to_owned());
    bucket.limit_label = Some("SGD 260.00".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec!["30% used", "Monthly cap: SGD 78.49 / SGD 260.00"]
    );
    assert_eq!(presentation.meter_percent, Some(30));
}

#[test]
fn usage_bucket_presentation_orders_non_spend_budget() {
    let mut bucket = presentation_bucket(
        "Global budget",
        None,
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("$0.00 spent".to_owned());
    bucket.limit_label = Some("$25,000.00".to_owned());
    bucket.used_money = Some(Money::new(0, "USD", 2));
    bucket.limit_money = Some(Money::new(2_500_000, "USD", 2));
    let presentation = usage_bucket_presentation(&bucket);
    assert!(
        presentation
            .display_segments
            .contains(&"Budget: $0.00 spent / $25,000.00".to_owned())
    );
}

#[test]
fn usage_bucket_presentation_appends_degraded_status() {
    let bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Stale,
    );
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.display_segments, vec!["57% left", "stale"]);
}

#[test]
fn usage_bucket_presentation_credits_zero_left() {
    let mut bucket = presentation_bucket("Credits", Some(0), None, UsageSnapshotStatus::Fresh);
    bucket.limit_label = Some("$4.76".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments.first().map(String::as_str),
        Some("0 left")
    );
    assert!(presentation.display_segments.contains(&"$4.76".to_owned()));
    assert_eq!(presentation.meter_percent, Some(0));
}

#[test]
fn usage_bucket_presentation_limit_only_balance() {
    let mut bucket = presentation_bucket("Prepaid", None, None, UsageSnapshotStatus::Fresh);
    bucket.limit_label = Some("$25".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.display_segments, vec!["$25"]);
    assert_eq!(presentation.meter_percent, None);
    assert_eq!(presentation.remaining_label, None);
}

// ===== Plan 004: Variant A run-out producer =====

#[test]
fn quota_pace_label_appends_runout_when_behind_pace() {
    // time_left=53%, delta=-5; elapsed=470, used=52; 48*470/52=433.85 -> 434s -> "7m"; 434 < 530.
    assert_eq!(
        quota_pace_label(Some(48), Some(10_530), Some(1_000), 10_000).expect("pace"),
        "5% in deficit · Runs out in 7m"
    );
    // Weekly-realistic 7-day window: 48*284401/52 = 262524s ~ 3d; 262524 < 320399.
    assert_eq!(
        quota_pace_label(Some(48), Some(320_399), Some(604_800), 0).expect("pace"),
        "5% in deficit · Runs out in 3d"
    );
}

#[test]
fn quota_pace_label_no_runout_when_ahead_of_pace() {
    // run-out would be 90*400/10 = 3600 >= 600 -> no segment.
    assert_eq!(
        quota_pace_label(Some(90), Some(600), Some(1_000), 0).expect("pace"),
        "30% in reserve"
    );
}

#[test]
fn quota_pace_label_no_runout_when_nothing_used() {
    // used == 0 -> returns without dividing (no division by zero).
    assert_eq!(
        quota_pace_label(Some(100), Some(500), Some(1_000), 0).expect("pace"),
        "50% in reserve"
    );
}

#[test]
fn quota_pace_label_no_runout_at_window_start() {
    // elapsed == 0 -> no segment even though delta = -40.
    assert_eq!(
        quota_pace_label(Some(60), Some(1_000), Some(1_000), 0).expect("pace"),
        "40% in deficit"
    );
}

#[test]
fn quota_pace_label_runout_iff_behind_clock_boundary() {
    // reset_at=500, window=1000, now=0.
    // delta=0 -> On pace; run-out 50*500/50=500, not strictly < 500 -> bare.
    assert_eq!(
        quota_pace_label(Some(50), Some(500), Some(1_000), 0).expect("pace"),
        "On pace"
    );
    // delta=+1 (ahead, in band); 51*500/49=520.4 -> 520 >= 500 -> bare.
    assert_eq!(
        quota_pace_label(Some(51), Some(500), Some(1_000), 0).expect("pace"),
        "On pace"
    );
    // delta=-1 (behind, in band); 49*500/51=480.4 -> 480s -> "8m"; 480 < 500.
    assert_eq!(
        quota_pace_label(Some(49), Some(500), Some(1_000), 0).expect("pace"),
        "On pace · Runs out in 8m"
    );
    // delta=-2 (band edge); 48*500/52=461.5 -> 462s -> "7m".
    assert_eq!(
        quota_pace_label(Some(48), Some(500), Some(1_000), 0).expect("pace"),
        "On pace · Runs out in 7m"
    );
    // delta=-3 (first deficit token); 47*500/53=443.4 -> 443s -> "7m".
    assert_eq!(
        quota_pace_label(Some(47), Some(500), Some(1_000), 0).expect("pace"),
        "3% in deficit · Runs out in 7m"
    );
}

#[test]
fn quota_pace_label_runout_depleted_bucket() {
    // used=100, elapsed=500, run-out=0 < 500 -> trivially precedes reset.
    assert_eq!(
        quota_pace_label(Some(0), Some(500), Some(1_000), 0).expect("pace"),
        "50% in deficit · Runs out in 0m"
    );
}

#[test]
fn quota_pace_label_exact_projection_precedes_reset_before_rounding() {
    // Exact 49*536/51 = 514.98… < 515; display rounding is 515 (would fail if
    // rounded seconds were compared to reset seconds).
    assert_eq!(
        quota_pace_label(Some(49), Some(10_515), Some(1_051), 10_000).expect("pace"),
        "On pace · Runs out in 8m"
    );
}

#[test]
fn quota_pace_label_exact_clock_equality_ignores_float_drift() {
    // 7*1000 == 70*100 -> projection reaches reset exactly -> no run-out segment.
    let label = quota_pace_label(Some(7), Some(70), Some(1_000), 0).expect("pace");
    assert!(!label.contains("Runs out"), "unexpected run-out: {label}");
}
