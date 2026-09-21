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
    assert_eq!(cap.used_label.as_deref(), Some("$12.50"));
    assert_eq!(cap.limit_label.as_deref(), Some("$100"));
    assert_eq!(cap.remaining_percent, Some(88));
    assert_eq!(cap.status_slot, Some(StatusSlot::Spend));
    let month = quota
        .buckets
        .iter()
        .find(|bucket| bucket.label == "Spent this month")
        .expect("monthly row");
    assert_eq!(month.used_label.as_deref(), Some("$12.50"));
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
fn openrouter_credits_403_is_typed_scope_denial() {
    assert_eq!(
        OpenRouterCreditsOutcome::from_http_status(403),
        OpenRouterCreditsOutcome::ManagementScopeDenied
    );
    assert!(matches!(
        OpenRouterCreditsOutcome::from_http_status(500),
        OpenRouterCreditsOutcome::Unavailable(_)
    ));
}

#[test]
fn openrouter_credits_separates_spend_from_remaining_balance() {
    let outcome = parse_openrouter_credits(serde_json::json!({
        "data": {"total_credits": 100.0, "total_usage": 25.0}
    }))
    .expect("credits fixture");
    let OpenRouterCreditsOutcome::Available {
        spent_cents,
        ceiling_cents,
    } = outcome
    else {
        panic!("expected available credits");
    };
    let view = openrouter_credits_bucket(spent_cents, ceiling_cents);
    assert_eq!(view.used_label.as_deref(), Some("$25"));
    assert_eq!(view.limit_label.as_deref(), Some("$100"));
    assert_eq!(
        view.used_money.as_ref().map(|money| money.amount_minor),
        Some(2_500)
    );
    assert_eq!(
        view.limit_money.as_ref().map(|money| money.amount_minor),
        Some(10_000)
    );
    assert_eq!(view.remaining_percent, Some(75));

    let exhausted = openrouter_credits_bucket(1_000, 1_000);
    assert_eq!(exhausted.used_label.as_deref(), Some("$10"));
    assert_eq!(exhausted.remaining_percent, Some(0));

    let overage = openrouter_credits_bucket(12_000, 10_000);
    assert_eq!(overage.used_label.as_deref(), Some("120% used"));
    assert_eq!(overage.remaining_percent, None);
    assert_eq!(
        overage.used_money.as_ref().map(|money| money.amount_minor),
        Some(12_000)
    );

    let bare = openrouter_credits_bucket(0, 0);
    assert_eq!(bare.remaining_percent, None);
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
    assert_eq!(cap.used_label.as_deref(), Some("$12.50"));
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
fn openrouter_snapshot_with_base_serves_key_quota_from_canned_api() {
    use std::io::{Read as _, Write as _};

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let address = listener.local_addr().unwrap();
    // One `/key` read; `/credits` is skipped unless the key read is Fresh,
    // and the fixture parses Fresh — so serve both.
    let server = std::thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).unwrap();
            let body = if String::from_utf8_lossy(&request[..read]).contains("/credits") {
                serde_json::json!({"data": {"total_credits": 10.0, "total_usage": 2.5}}).to_string()
            } else {
                key_fixture().to_string()
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let view = openrouter_snapshot_with_base(
        "opencode",
        Some("fixture-key"),
        &format!("http://{address}"),
        1_780_000_000,
    );
    server.join().unwrap();

    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.account.provider_label, "OpenRouter");
    assert!(
        view.buckets
            .iter()
            .any(|bucket| bucket.label == "Key Limit")
    );
}

#[test]
fn openrouter_snapshot_with_controlled_connection_error_is_error() {
    let view = openrouter_snapshot_with_key_fetch(
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
        assert_eq!(
            openrouter_key_error_status(failure),
            UsageSnapshotStatus::Error,
            "non-HTTP-status failure must not become NeedsLogin: {failure}"
        );
    }
    assert_eq!(
        openrouter_key_error_status(&ProviderHttpError::HttpStatus {
            status: 401,
            message: "OpenRouter key HTTP 401 Unauthorized".to_owned(),
        }),
        UsageSnapshotStatus::NeedsLogin
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
