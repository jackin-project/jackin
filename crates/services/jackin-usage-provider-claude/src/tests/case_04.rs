// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::cell::Cell;

fn resolved_for_test() -> ClaudeResolved {
    ClaudeResolved {
        access_token: "fixture-token".to_owned(),
        subscription_type: Some("Claude Max".to_owned()),
        account_email: Some("operator@example.test".to_owned()),
        organization_type: None,
        credential_origin: "OAuth · fixture".to_owned(),
        is_anonymous: false,
    }
}

#[test]
fn classify_claude_keychain_status_distinguishes_denial_absence_and_consent() {
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
        ClaudeKeychainRead::ConsentRequired
    ));
    assert!(matches!(
        classify_claude_keychain_status(-1),
        ClaudeKeychainRead::Missing
    ));
}

#[test]
fn claude_wave_policy_is_typed_and_does_not_expose_secret() {
    let shared = ClaudeWaveResolution::Resolved(Box::new(resolved_for_test()));
    assert_eq!(claude_wave_policy(&shared), ClaudeWavePolicy::Shared);

    let anonymous = ClaudeWaveResolution::Resolved(Box::new(ClaudeResolved {
        is_anonymous: true,
        ..resolved_for_test()
    }));
    assert_eq!(
        claude_wave_policy(&anonymous),
        ClaudeWavePolicy::LocalAnonymous
    );
    assert_eq!(
        claude_wave_policy(&ClaudeWaveResolution::Denied),
        ClaudeWavePolicy::LocalDenied
    );
    assert_eq!(
        claude_wave_policy(&ClaudeWaveResolution::Missing),
        ClaudeWavePolicy::LocalMissing
    );
}

#[test]
fn claude_http_auth_scope_and_rate_failures_keep_typed_status() {
    for (http_status, expected_snapshot_status) in [
        (401, UsageSnapshotStatus::NeedsLogin),
        (403, UsageSnapshotStatus::Stale),
        (429, UsageSnapshotStatus::Stale),
    ] {
        let calls = Cell::new(0);
        let now = 1_781_185_560;
        let (view, rate_limit, provider_error) = claude_resolved_view_with_fetch(
            "claude",
            Some("Anthropic / Claude"),
            now,
            resolved_for_test(),
            |token| {
                assert_eq!(token, "fixture-token");
                calls.set(calls.get() + 1);
                Err(ProviderHttpError::HttpStatus {
                    status: http_status,
                    message: format!("Claude OAuth usage HTTP {http_status}"),
                    retry_after_seconds: (http_status == 429).then_some(300),
                    response_received_at_epoch: Some(now),
                })
            },
        );

        assert_eq!(calls.get(), 1, "one OAuth attempt for HTTP {http_status}");
        assert_eq!(view.status, expected_snapshot_status);
        assert_eq!(
            provider_error.map(|error| (error.kind, error.http_status)),
            Some((
                jackin_usage_provider_core::ProviderErrorKind::HttpStatus,
                Some(http_status)
            ))
        );
        assert!(
            view.buckets
                .iter()
                .all(|bucket| bucket.status == expected_snapshot_status)
        );
        let last_error = view.last_error.as_deref().expect("typed error is retained");
        assert!(last_error.contains(&format!("HTTP {http_status}")));

        if http_status == 403 {
            assert!(last_error.contains("inference-only"));
        }
        if http_status == 429 {
            assert_eq!(
                rate_limit.and_then(|limit| limit.retry_at_epoch),
                Some(now + 300)
            );
        } else {
            assert_eq!(rate_limit, None);
        }
    }
}

#[test]
fn claude_401_does_not_reread_credentials_and_uses_only_later_caller_token() {
    let old_token_calls = Cell::new(0);
    let (old_view, _, old_error) = claude_resolved_view_with_fetch(
        "claude",
        Some("Anthropic / Claude"),
        1_781_185_560,
        resolved_for_test(),
        |token| {
            assert_eq!(token, "fixture-token");
            old_token_calls.set(old_token_calls.get() + 1);
            Err(ProviderHttpError::HttpStatus {
                status: 401,
                message: "Claude OAuth usage HTTP 401".to_owned(),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            })
        },
    );

    assert_eq!(
        old_token_calls.get(),
        1,
        "401 does not trigger another fetch"
    );
    assert_eq!(old_view.status, UsageSnapshotStatus::NeedsLogin);
    assert_eq!(
        old_error.map(|error| (error.kind, error.http_status)),
        Some((
            jackin_usage_provider_core::ProviderErrorKind::HttpStatus,
            Some(401)
        ))
    );

    // A later broker invocation may supply a changed credential. The provider
    // has no source handle to reread, so this is a separate caller-supplied
    // token, not an automatic same-source retry after 401.
    let new_token_calls = Cell::new(0);
    let mut new_credential = resolved_for_test();
    new_credential.access_token = "new-fixture-token".to_owned();
    let (_, _, new_error) = claude_resolved_view_with_fetch(
        "claude",
        Some("Anthropic / Claude"),
        1_781_185_560,
        new_credential,
        |token| {
            assert_eq!(token, "new-fixture-token");
            new_token_calls.set(new_token_calls.get() + 1);
            Ok(serde_json::from_value(serde_json::json!({}))
                .expect("empty fake usage response decodes"))
        },
    );

    assert_eq!(new_token_calls.get(), 1);
    assert_eq!(new_error, None);
}

#[test]
fn claude_http_timeout_and_transport_failures_stay_typed_and_do_not_retry() {
    for (failure, expected_kind, message) in [
        (
            ProviderHttpError::Timeout("Claude OAuth usage request timed out".to_owned()),
            jackin_usage_provider_core::ProviderErrorKind::Timeout,
            "Claude OAuth usage request timed out",
        ),
        (
            ProviderHttpError::Transport("Claude OAuth usage connection reset".to_owned()),
            jackin_usage_provider_core::ProviderErrorKind::Transport,
            "Claude OAuth usage connection reset",
        ),
    ] {
        let calls = Cell::new(0);
        let (view, rate_limit, provider_error) = claude_resolved_view_with_fetch(
            "claude",
            Some("Anthropic / Claude"),
            1_781_185_560,
            resolved_for_test(),
            |_| {
                calls.set(calls.get() + 1);
                Err(failure)
            },
        );

        assert_eq!(calls.get(), 1);
        assert_eq!(view.status, UsageSnapshotStatus::Stale);
        assert_eq!(
            provider_error.map(|error| (error.kind, error.http_status)),
            Some((expected_kind, None))
        );
        assert_eq!(view.last_error.as_deref(), Some(message));
        assert_eq!(rate_limit, None);
    }
}

#[test]
fn claude_denied_view_has_no_quota_and_exact_error() {
    let (view, rate_limit, provider_error) = claude_view_from_wave(
        "claude",
        Some("Anthropic / Claude"),
        1_781_185_560,
        ClaudeWaveResolution::Denied,
    );
    assert_eq!(rate_limit, None);
    assert_eq!(provider_error, None);
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

#[test]
fn claude_limits_inactive_flag_does_not_gate_rendering() {
    // Live responses send `is_active: false` on headline limits that still
    // carry quota — the flag must never suppress a bucket.
    let response: ClaudeOAuthUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": null,
        "seven_day": null,
        "limits": [
            {"kind": "session", "percent": 10, "is_active": false,
             "resets_at": "2026-09-17T10:00:00Z"},
            {"kind": "weekly_all", "percent": 42, "is_active": false,
             "resets_at": "2026-09-24T10:00:00Z"},
        ]
    }))
    .expect("inactive limits decode");
    let buckets = response.into_buckets(1_781_185_560);
    let session = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Session))
        .expect("session bucket despite is_active false");
    assert_eq!(session.label, "Session");
    assert_eq!(session.remaining_percent, Some(90));
    let weekly = buckets
        .iter()
        .find(|bucket| bucket.status_slot == Some(StatusSlot::Weekly))
        .expect("weekly bucket despite is_active false");
    assert_eq!(weekly.label, "All models");
    assert_eq!(weekly.remaining_percent, Some(58));
}
