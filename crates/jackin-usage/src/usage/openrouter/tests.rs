// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn key_fixture() -> serde_json::Value {
    serde_json::json!({
        "data": {
            "label": "test-key",
            "usage": 12.5,
            "usage_daily": 1.2,
            "usage_weekly": 5.0,
            "usage_monthly": 12.5,
            "limit": 100.0,
            "limit_remaining": 87.5,
            "is_free_tier": false
        }
    })
}

#[test]
fn openrouter_key_maps_cap_remaining_and_period_spend() {
    let quota = parse_openrouter_key_usage(key_fixture(), 1_780_000_000).expect("key fixture");
    assert_eq!(quota.plan_label.as_deref(), Some("Pay as you go"));
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("cap row");
    assert_eq!(cap.used_label.as_deref(), Some("$12.5"));
    assert_eq!(cap.limit_label.as_deref(), Some("$100"));
    assert_eq!(cap.remaining_percent, Some(87));
    assert_eq!(cap.status_slot, Some(StatusSlot::Spend));
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly row");
    assert_eq!(month.used_label.as_deref(), Some("$12.5"));
    assert_eq!(month.remaining_percent, None);
    assert_eq!(
        quota
            .buckets
            .iter()
            .filter(|bucket| bucket.label.starts_with("Spent"))
            .count(),
        3
    );
}

#[test]
fn openrouter_null_cap_means_no_cap_never_infinite() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {
                "usage_monthly": 12.5,
                "limit": null,
                "limit_remaining": null,
                "is_free_tier": true
            }
        }),
        1_780_000_000,
    )
    .expect("null cap");
    assert_eq!(quota.plan_label.as_deref(), Some("Free tier"));
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.label != "Key Limit"),
        "no cap row without a configured cap"
    );
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.remaining_percent.is_none()),
        "no percentage without a denominator"
    );
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly spend still renders");
    assert_eq!(month.status_slot, Some(StatusSlot::Spend));
}

#[test]
fn openrouter_cap_without_remaining_shows_limit_only() {
    let quota =
        parse_openrouter_key_usage(serde_json::json!({"data": {"limit": 50.0}}), 1_780_000_000)
            .expect("cap without remaining");
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("cap row");
    assert_eq!(cap.limit_label.as_deref(), Some("$50"));
    assert_eq!(cap.used_label, None);
    assert_eq!(cap.remaining_percent, None);
}

#[test]
fn openrouter_byok_stays_a_separate_row() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {
                "usage_monthly": 12.5,
                "limit": 100.0,
                "limit_remaining": 87.5,
                "byok_usage_monthly": 3.0
            }
        }),
        1_780_000_000,
    )
    .expect("byok fixture");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("byok row");
    assert_eq!(byok.used_label.as_deref(), Some("$3"));
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("cap row");
    assert_eq!(cap.used_label.as_deref(), Some("$12.5"));
}

#[test]
fn openrouter_model_catalog_omission_is_unverified_not_rejection() {
    let catalog = serde_json::json!({"data": [{"id": "openrouter/auto"}]});
    assert!(matches!(
        check_openrouter_model_in_catalog(&catalog, "openrouter/auto"),
        OpenRouterModelCheck::Verified { .. }
    ));
    let stale = check_openrouter_model_in_catalog(&catalog, "acme/new-model");
    assert!(matches!(stale, OpenRouterModelCheck::Unverified { .. }));
    let broken = check_openrouter_model_in_catalog(&serde_json::json!({}), "acme/new-model");
    assert!(matches!(broken, OpenRouterModelCheck::Unverified { .. }));
}

#[test]
fn openrouter_base_url_prefers_api_url_override() {
    // Hermetic: the pure resolver, so live env can never vacate this test.
    assert_eq!(
        openrouter_base_url_from(None, None),
        OPENROUTER_DEFAULT_BASE_URL
    );
    assert_eq!(
        openrouter_base_url_from(Some("https://api.test/"), Some("https://base.test")),
        "https://api.test"
    );
    assert_eq!(
        openrouter_base_url_from(None, Some("https://base.test/")),
        "https://base.test"
    );
    assert_eq!(
        openrouter_base_url_from(Some("  "), Some("https://base.test")),
        "https://base.test"
    );
}

#[test]
fn openrouter_byok_fallback_ignores_caps() {
    // A `byok_limit` cap is not spend: the fallback must not read it.
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {
                "usage_monthly": 12.5,
                "byok_limit": 100.0,
                "byok_remaining": 40.0
            }
        }),
        1_780_000_000,
    )
    .expect("byok caps fixture");
    assert!(
        quota
            .buckets
            .iter()
            .all(|bucket| bucket.label != "BYOK spend"),
        "caps never become a spend row"
    );
    // …while a genuinely spend-named fallback key still counts.
    let quota = parse_openrouter_key_usage(
        serde_json::json!({
            "data": {"usage_monthly": 12.5, "byok_spend_total": 3.0}
        }),
        1_780_000_000,
    )
    .expect("byok spend fixture");
    let byok = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "BYOK spend")
        .expect("byok row");
    assert_eq!(byok.used_label.as_deref(), Some("$3"));
}

#[test]
fn openrouter_omitted_tier_flag_is_unknown_plan() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({"data": {"usage_monthly": 1.0}}),
        1_780_000_000,
    )
    .expect("tierless fixture");
    assert_eq!(quota.plan_label, None);
}

#[test]
fn openrouter_snapshot_without_key_needs_login() {
    let view = openrouter_snapshot("opencode", None, 1_780_000_000);
    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.account.provider_label, "OpenRouter");
}

#[test]
fn openrouter_inference_snapshot_never_calls_management_routes() {
    use std::io::{Read as _, Write as _};

    // Hold the listener until the synchronous snapshot completes. Record and
    // answer every request, so the broken collector fails on the forbidden
    // route assertion instead of hanging or failing to connect.
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut routes = Vec::new();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                        .unwrap();
                    let mut request = [0_u8; 4096];
                    let read = stream.read(&mut request).unwrap();
                    let request = String::from_utf8_lossy(&request[..read]);
                    let route = request.split_whitespace().nth(1).unwrap().to_owned();
                    let (status, body) = if route == "/key" {
                        ("200 OK", key_fixture().to_string())
                    } else {
                        ("403 Forbidden", "management key required".to_owned())
                    };
                    routes.push(route);
                    write!(stream,
                        "HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()).unwrap();
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    match done_rx.recv_timeout(std::time::Duration::from_millis(5)) {
                        Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    }
                }
                Err(error) => panic!("fixture accept: {error}"),
            }
        }
        routes
    });
    let view = openrouter_snapshot_with_base(
        "opencode",
        Some("fixture-key"),
        &format!("http://{address}"),
        1_780_000_000,
    );
    done_tx.send(()).unwrap();
    assert_eq!(server.join().unwrap(), vec!["/key"]);
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.account.provider_label, "OpenRouter");
    assert!(
        view.buckets
            .iter()
            .any(|bucket| bucket.label == "Key Limit")
    );
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|note| note.contains("Management"))
    );
    assert!(
        view.buckets
            .iter()
            .all(|bucket| bucket.label != "Account Credits")
    );
}

#[test]
fn openrouter_configured_zero_cap_stays_explicit() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({"data": {"limit": 0, "limit_remaining": 0}}),
        1_780_000_000,
    )
    .unwrap();
    let cap = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Key Limit")
        .expect("zero cap row");
    assert_eq!(
        cap.limit_money.as_ref().map(|money| money.amount_minor),
        Some(0)
    );
    assert_eq!(
        cap.used_money.as_ref().map(|money| money.amount_minor),
        Some(0)
    );
    assert_eq!(cap.remaining_percent, None);
}

#[test]
fn openrouter_free_daily_request_quota_has_utc_window() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({"data": {
            "limit": null,
            "limit_remaining": null,
            "free_model_daily_requests": {"used": 12, "limit": 50, "remaining": 38}
        }}),
        1_780_000_000,
    )
    .unwrap();
    let daily = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Free model daily requests")
        .expect("daily quota");
    assert_eq!(daily.used_label.as_deref(), Some("12 requests"));
    assert_eq!(daily.limit_label.as_deref(), Some("50 requests"));
    assert_eq!(daily.remaining_percent, Some(76));
    assert_eq!(daily.resets_at, Some(1_780_012_800));
    assert_eq!(daily.used_money, None);
    assert_eq!(daily.limit_money, None);
    assert!(
        daily
            .pace_label
            .as_deref()
            .is_some_and(|note| note.contains("exempt"))
    );
}

#[test]
fn openrouter_free_daily_zero_allowance_is_not_unknown() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({"data": {"free_model_daily_requests": {"used": 0, "limit": 0, "remaining": 0}}}),
        1_780_000_000,
    ).unwrap();
    let daily = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Free model daily requests")
        .expect("zero daily quota");
    assert_eq!(daily.limit_label.as_deref(), Some("0 requests"));
    assert_eq!(daily.remaining_percent, None);
}

#[test]
fn openrouter_snapshot_with_controlled_connection_error_is_error() {
    let (view, rate_limit) = openrouter_snapshot_with_key_fetch(
        "opencode",
        Some("fixture-key"),
        "http://127.0.0.1:40101",
        1_780_000_000,
        |_base_url, _key| {
            Err(ProviderHttpError::Transport(
                "OpenRouter key request failed for http://127.0.0.1:40101/key: connection refused"
                    .to_owned(),
            ))
        },
    );

    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert_eq!(rate_limit, None);
    assert_eq!(view.account.provider_label, "OpenRouter");
    assert_eq!(
        view.last_error.as_deref(),
        Some("OpenRouter key request failed for http://127.0.0.1:40101/key: connection refused")
    );
}

#[test]
fn openrouter_transport_text_and_url_401_are_not_auth_failures() {
    let failures = [
        ProviderHttpError::Transport(
            "OpenRouter key request failed: transport reported status 401".to_owned(),
        ),
        ProviderHttpError::Transport(
            "OpenRouter key request failed for http://127.0.0.1:40101/key".to_owned(),
        ),
        ProviderHttpError::Decode("OpenRouter key decode failed: payload mentions 401".to_owned()),
    ];

    for failure in &failures {
        let typed = ProviderError::from(failure.clone());
        assert_eq!(
            openrouter_key_error_status(&typed),
            UsageSnapshotStatus::Error,
            "non-HTTP-status failure must not become NeedsLogin: {failure}"
        );
    }
    assert_eq!(
        openrouter_key_error_status(&ProviderError::from(ProviderHttpError::HttpStatus {
            status: 401,
            message: "OpenRouter key HTTP 401 Unauthorized".to_owned(),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        })),
        UsageSnapshotStatus::NeedsLogin
    );
}

#[expect(
    clippy::disallowed_methods,
    reason = "test-only delayed HTTP fixture runs on an owned OS helper thread"
)]
#[test]
fn openrouter_snapshot_carries_delayed_429_retry_after_to_broker_boundary() {
    use std::io::{Read as _, Write as _};
    use std::time::Duration;

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("429 fixture accept");
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).expect("429 fixture read");
        assert!(
            String::from_utf8_lossy(&request[..read]).contains("/key"),
            "fixture must receive the key request"
        );
        std::thread::sleep(Duration::from_millis(1_100));
        let body = "{\"error\":\"provider body mentions 429\"}";
        write!(
            stream,
            "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 37\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("429 fixture write");
    });

    let request_now = now_epoch().saturating_sub(120);
    let (view, rate_limit) = openrouter_snapshot_with_key_fetch(
        "opencode",
        Some("fixture-key"),
        &format!("http://{address}"),
        request_now,
        fetch_openrouter_key_usage,
    );
    server.join().expect("429 fixture server");

    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|message| message.contains("429")),
        "typed HTTP status should remain visible in the snapshot message"
    );
    let retry_at = rate_limit
        .expect("429 must reach the rate-limit boundary")
        .retry_at_epoch
        .expect("numeric Retry-After must produce an absolute deadline");
    assert!(
        retry_at >= request_now + 100,
        "deadline must start from response receipt, not request start: retry_at={retry_at}, request_now={request_now}"
    );
}

fn one_shot_key_server(status: u16, body: &str) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let body = body.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 4096];
        let read = stream.read(&mut request).unwrap();
        assert!(String::from_utf8_lossy(&request[..read]).contains("/key"));
        let reason = match status {
            200 => "OK",
            401 => "Unauthorized",
            _ => "Error",
        };
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    (format!("http://{address}"), server)
}

#[test]
fn openrouter_actual_401_is_needs_login() {
    let (base_url, server) = one_shot_key_server(401, "unauthorized");
    let view =
        openrouter_snapshot_with_base("opencode", Some("fixture-key"), &base_url, 1_780_000_000);
    server.join().unwrap();

    assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(
        view.last_error.as_deref(),
        Some("OpenRouter key HTTP 401 Unauthorized")
    );
}

#[test]
fn openrouter_malformed_key_payload_is_error() {
    let (base_url, server) = one_shot_key_server(200, "not-json");
    let view =
        openrouter_snapshot_with_base("opencode", Some("fixture-key"), &base_url, 1_780_000_000);
    server.join().unwrap();

    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| error.starts_with("OpenRouter key decode failed:")),
        "malformed payload must retain decode failure"
    );
}

#[test]
fn openrouter_actual_403_is_error_not_login_or_rate_limit() {
    let (base_url, server) = one_shot_key_server(403, "forbidden");
    let (view, rate_limit) = openrouter_snapshot_with_key_fetch(
        "opencode",
        Some("fixture-key"),
        &base_url,
        1_780_000_000,
        fetch_openrouter_key_usage,
    );
    server.join().unwrap();
    assert_eq!(view.status, UsageSnapshotStatus::Error);
    assert_eq!(rate_limit, None);
    assert!(
        view.last_error
            .as_deref()
            .is_some_and(|error| error.contains("403"))
    );
}

#[test]
fn openrouter_negative_request_counter_is_domain_error() {
    assert!(parse_openrouter_key_usage(
        serde_json::json!({"data": {"free_model_daily_requests": {"used": -1, "limit": 50, "remaining": 50}}}),
        1_780_000_000,
    ).is_err());
}

#[test]
fn openrouter_daily_reset_at_midnight_is_next_utc_day() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({"data": {"free_model_daily_requests": {"used": 0, "limit": 50, "remaining": 50}}}),
        86_400,
    ).unwrap();
    let daily = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Free model daily requests")
        .unwrap();
    assert_eq!(daily.resets_at, Some(172_800));
}

#[test]
fn openrouter_daily_counts_preserve_partial_unknown_and_full_u64_domain() {
    let partial = parse_openrouter_key_usage(
        serde_json::json!({"data": {"free_model_daily_requests": {"used": 12}}}),
        1_780_000_000,
    )
    .unwrap();
    let daily = partial
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Free model daily requests")
        .unwrap();
    let counts = daily.count_quota.as_ref().unwrap();
    assert_eq!(counts.used, Some(12));
    assert_eq!(counts.limit, None);
    assert_eq!(counts.remaining, None);
    assert_eq!(daily.remaining_percent, None);
    let label = usage_bucket_presentation(daily).display_label;
    assert!(label.contains("12 requests used; limit unknown"));
    assert!(label.contains("Remaining requests unknown"));

    let full = parse_openrouter_key_usage(
        serde_json::json!({"data": {"free_model_daily_requests": {
            "used": 18446744073709551615_u64,
            "limit": 18446744073709551615_u64,
            "remaining": 1
        }}}),
        1_780_000_000,
    )
    .unwrap();
    let daily = full
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Free model daily requests")
        .unwrap();
    let counts = daily.count_quota.as_ref().unwrap();
    assert_eq!(counts.used, Some(u64::MAX));
    assert_eq!(counts.limit, Some(u64::MAX));
    assert_eq!(counts.remaining, Some(1));
    assert_eq!(daily.remaining_percent, Some(0));
    let label = usage_bucket_presentation(daily).display_label;
    assert!(label.contains("18446744073709551615 / 18446744073709551615 requests used"));
    assert!(label.contains("1 requests left"));
}

#[test]
fn openrouter_daily_counts_preserve_independent_remaining_on_stale_presentation() {
    let quota = parse_openrouter_key_usage(
        serde_json::json!({"data": {"free_model_daily_requests": {"used": 12, "limit": 50, "remaining": 37}}}),
        1_780_000_000,
    ).unwrap();
    let mut daily = quota
        .buckets
        .into_iter()
        .find(|bucket| bucket.label == "Free model daily requests")
        .unwrap();
    daily.status = UsageSnapshotStatus::Stale;
    let presentation = usage_bucket_presentation(&daily);
    assert!(presentation.display_label.contains("12 / 50 requests used"));
    assert!(presentation.display_label.contains("37 requests left"));
    assert!(presentation.display_label.contains("stale"));
    assert_eq!(daily.count_quota.as_ref().unwrap().remaining, Some(37));
}

#[test]
fn openrouter_recurring_key_cap_uses_documented_utc_boundaries() {
    for (policy, expected_reset) in [
        ("daily", Some(1_780_358_400)),
        ("weekly", Some(1_780_876_800)),
        ("monthly", Some(1_782_864_000)),
        ("unknown", None),
    ] {
        let quota = parse_openrouter_key_usage(
            serde_json::json!({"data": {"limit": 50, "limit_remaining": 38, "limit_reset": policy}}),
            1_780_272_000, // 2026-06-01 00:00:00 UTC, Monday.
        ).unwrap();
        let cap = quota
            .buckets
            .iter()
            .find(|bucket| bucket.label == "Key Limit")
            .unwrap();
        assert_eq!(cap.resets_at, expected_reset, "{policy}");
    }
    assert_eq!(
        openrouter_key_reset(
            &serde_json::json!({"limit_reset": "monthly"}),
            1_798_761_599
        ),
        Some(1_798_761_600)
    );
    assert_eq!(
        openrouter_key_reset(&serde_json::json!({"limit_reset": null}), 1_780_272_000),
        None
    );
    assert_eq!(
        openrouter_key_reset(&serde_json::json!({"limit_reset": "monthly"}), i64::MAX),
        None
    );
}
