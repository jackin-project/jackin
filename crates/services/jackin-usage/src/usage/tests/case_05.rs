// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn broker_client_failure_preserves_last_good_quota() {
    let target = UsageRefreshTarget {
        agent: "claude".to_owned(),
        provider: Some("Claude".to_owned()),
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "account-claude".to_owned(),
            surface_id: "claude".to_owned(),
        },
    };
    let mut cached = FocusedUsageView::unavailable("seed", 123);
    cached.status = UsageSnapshotStatus::Fresh;
    cached.buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
        label: "Weekly".to_owned(),
        used_label: Some("36% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(64),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Weekly),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
    }];
    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_capability_for_test(
        "claude",
        Some("Claude"),
        &target.capability,
        cached,
    );

    cache.adopt_broker_error(
        &target,
        &jackin_protocol::usage_broker::UsageCoordinationError {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::Unavailable,
            message: "usage broker is unavailable".to_owned(),
        },
    );

    let adopted = cache.focused_snapshot_for_capability(
        Some("claude"),
        Some("Claude"),
        Some(&target.capability),
    );
    assert_eq!(adopted.status, UsageSnapshotStatus::Stale);
    assert_eq!(adopted.buckets[0].remaining_percent, Some(64));
    assert_eq!(adopted.buckets[0].status, UsageSnapshotStatus::Stale);
    assert_eq!(
        cache
            .snapshots
            .values()
            .next()
            .map(|cached| cached.view.updated_label.as_str()),
        Some("Stale")
    );
    assert_eq!(
        adopted.last_error.as_deref(),
        Some("usage broker is unavailable")
    );
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
