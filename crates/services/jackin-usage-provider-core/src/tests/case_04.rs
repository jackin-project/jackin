// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_cache_adopts_broker_generations_by_account_capability() {
    let target = UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("OpenAI".to_owned()),
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "codex".to_owned(),
        },
    };
    let generation = |account_id: &str, account_label: &str| {
        let mut view = codex_cached_usage_view();
        view.account.account_label = account_label.to_owned();
        jackin_protocol::usage_broker::UsageGenerationView {
            capability: jackin_protocol::usage_broker::UsageAccountCapability {
                account_id: account_id.to_owned(),
                surface_id: "codex".to_owned(),
            },
            generation: 1,
            phase: jackin_protocol::usage_broker::UsageRefreshPhase::Completed,
            snapshot: Some(view),
            error: None,
            retry_at_epoch: None,
        }
    };

    let mut cache = UsageCache::default();
    cache.adopt_broker_generation(&target, &generation("account-a", "personal@example.test"));
    let other_target = UsageRefreshTarget {
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "account-b".to_owned(),
            surface_id: "codex".to_owned(),
        },
        ..target.clone()
    };
    cache.adopt_broker_generation(&other_target, &generation("account-b", "work@example.test"));

    assert_eq!(cache.snapshots.len(), 2);
    assert_eq!(cache.account_snapshot_views().len(), 2);
    cache.adopt_broker_error(
        &target,
        &jackin_protocol::usage_broker::UsageCoordinationError {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::ProviderUnavailable,
            message: "provider unavailable".to_owned(),
        },
    );
    assert_eq!(
        cache
            .snapshots
            .values()
            .find(|cached| cached.view.account.account_label == "personal@example.test")
            .map(|cached| cached.view.status),
        Some(UsageSnapshotStatus::Stale)
    );
    assert_eq!(
        cache
            .snapshots
            .values()
            .find(|cached| cached.view.account.account_label == "work@example.test")
            .map(|cached| cached.view.status),
        Some(UsageSnapshotStatus::Fresh)
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
fn unauthorized_errors_are_distinguished_from_transient() {
    for status in [401, 403] {
        assert!(usage_error_is_unauthorized(&ProviderError::from(
            ProviderHttpError::HttpStatus {
                status,
                message: format!("HTTP {status}"),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            },
        )));
    }
    assert!(!usage_error_is_unauthorized(&ProviderError::from(
        ProviderHttpError::Transport("request failed: HTTP 401".to_owned()),
    )));
    assert!(!usage_error_is_unauthorized(&ProviderError::from(
        ProviderHttpError::Decode("payload mentions 403".to_owned()),
    )));
    // A rate-limit is transient, not an auth failure.
    assert!(!usage_error_is_unauthorized(&ProviderError::from(
        ProviderHttpError::HttpStatus {
            status: 429,
            message: "usage HTTP 429 rate limit".to_owned(),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        },
    )));
}

#[test]
fn typed_rate_limit_preserves_retry_after_but_rendered_429_text_does_not() {
    let typed = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 429,
        message: "provider response body mentions 429".to_owned(),
        retry_after_seconds: Some(37),
        response_received_at_epoch: Some(1_700_000_000),
    });
    assert!(usage_error_is_rate_limited(&typed));
    assert_eq!(typed.retry_after_seconds(), Some(37));
    assert_eq!(
        typed.rate_limit(),
        Some(ProviderRateLimit {
            retry_at_epoch: Some(1_700_000_037),
        })
    );

    for error in [
        ProviderError::from(ProviderHttpError::Transport(
            "transport failed after HTTP 429".to_owned(),
        )),
        ProviderError::from(ProviderHttpError::Decode(
            "decode failed: payload mentions 429 and Retry-After: 37".to_owned(),
        )),
    ] {
        assert!(!usage_error_is_rate_limited(&error));
        assert_eq!(error.retry_after_seconds(), None);
        assert_eq!(error.rate_limit(), None);
    }
}

#[test]
fn retry_after_accepts_delay_seconds_and_http_dates_against_response_time() {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::RETRY_AFTER,
        reqwest::header::HeaderValue::from_static(" 37 "),
    );
    assert_eq!(
        retry_after_header_seconds(&headers, 1_445_412_400),
        Some(37)
    );

    headers.insert(
        reqwest::header::RETRY_AFTER,
        reqwest::header::HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
    );
    let delay = retry_after_header_seconds(&headers, 1_445_412_400);
    assert_eq!(delay, Some(80));
    let typed = ProviderError::from(ProviderHttpError::HttpStatus {
        status: 429,
        message: "provider HTTP 429".to_owned(),
        retry_after_seconds: delay,
        response_received_at_epoch: Some(1_445_412_400),
    });
    assert_eq!(
        typed.rate_limit(),
        Some(ProviderRateLimit {
            retry_at_epoch: Some(1_445_412_480),
        })
    );

    assert_eq!(retry_after_header_value("invalid", 1_445_412_400), None);
    assert_eq!(retry_after_header_value("37.5", 1_445_412_400), None);
    assert_eq!(retry_after_header_value("-1", 1_445_412_400), None);
    assert_eq!(
        retry_after_header_value("Wed, 21 Oct 2015 07:28:00 GMT", 1_445_412_500),
        Some(0)
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
