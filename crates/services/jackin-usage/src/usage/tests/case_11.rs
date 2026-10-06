// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn grok_rest_contract_accepts_current_and_legacy_top_level_shapes() {
    let current = serde_json::json!({
        "creditUsagePercent": 12.5,
        "currentPeriod": {"type": "WEEKLY", "start": "2026-08-17T00:00:00Z", "end": "2026-08-24T00:00:00Z"}
    });
    let response = parse_grok_rest_billing_response(&current).expect("current REST shape");
    assert_eq!(response.buckets(1_755_000_000)[0].label, "Weekly");
    let legacy = serde_json::json!({
        "monthlyLimit": {"val": 30000}, "used": {"val": 5000},
        "billingPeriodStart": "2026-08-01T00:00:00Z", "billingPeriodEnd": "2026-09-01T00:00:00Z"
    });
    let response = parse_grok_rest_billing_response(&legacy).expect("legacy REST shape");
    assert_eq!(response.buckets(1_754_000_000)[0].label, "Monthly");
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
