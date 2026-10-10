// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::cell::{Cell, RefCell};

fn resolved_for_test() -> ClaudeResolved {
    ClaudeResolved::from_token(
        "fixture-token".to_owned(),
        Some("Claude Max".to_owned()),
        Some("operator@example.test".to_owned()),
        None,
        "OAuth · fixture".to_owned(),
        false,
    )
}

struct FakeClaudeCredentialSource {
    label: &'static str,
    token: RefCell<String>,
    read_count: Cell<usize>,
}

impl FakeClaudeCredentialSource {
    fn new(token: &str) -> Self {
        Self {
            label: "fixture-keychain-source",
            token: RefCell::new(token.to_owned()),
            read_count: Cell::new(0),
        }
    }

    fn resolve_explicitly(&self) -> ClaudeResolved {
        self.read_count.set(self.read_count.get() + 1);
        let resolved = resolved_for_test();
        ClaudeResolved::from_token(
            self.token.borrow().clone(),
            resolved.subscription_type,
            resolved.account_email,
            resolved.organization_type,
            self.label.to_owned(),
            resolved.is_anonymous,
        )
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

    let anonymous = ClaudeWaveResolution::Resolved(Box::new(ClaudeResolved::from_token(
        "fixture-token".to_owned(),
        Some("Claude Max".to_owned()),
        Some("operator@example.test".to_owned()),
        None,
        "OAuth · fixture".to_owned(),
        true,
    )));
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
fn claude_401_does_not_implicitly_reread_same_source_or_retry() {
    const NOW: i64 = 1_781_185_560;

    for mutate_source_during_401 in [false, true] {
        let source = FakeClaudeCredentialSource::new("fixture-token");
        let initially_resolved = source.resolve_explicitly();
        assert_eq!(source.read_count.get(), 1);
        assert_eq!(initially_resolved.credential_origin, source.label);

        let first_fetches = Cell::new(0);
        let (view, rate_limit, provider_error) = claude_resolved_view_with_fetch(
            "claude",
            Some("Anthropic / Claude"),
            NOW,
            initially_resolved,
            |token| {
                assert_eq!(token, "fixture-token");
                first_fetches.set(first_fetches.get() + 1);
                if mutate_source_during_401 {
                    *source.token.borrow_mut() = "changed-after-401".to_owned();
                }
                Err(ProviderHttpError::HttpStatus {
                    status: 401,
                    message: "Claude OAuth usage HTTP 401".to_owned(),
                    retry_after_seconds: None,
                    response_received_at_epoch: Some(NOW),
                })
            },
        );

        assert_eq!(first_fetches.get(), 1, "401 must not retry the HTTP fetch");
        assert_eq!(source.read_count.get(), 1, "401 must not reread its source");
        assert_eq!(view.status, UsageSnapshotStatus::NeedsLogin);
        assert_eq!(rate_limit, None);
        assert_eq!(
            provider_error.map(|error| (error.kind, error.http_status)),
            Some((
                jackin_usage_provider_core::ProviderErrorKind::HttpStatus,
                Some(401)
            ))
        );

        // The contract permits at most one noninteractive reread, so zero is
        // valid here: this factory receives only a resolved credential and has
        // no source handle. A later explicit caller resolution uses the same
        // fake source, whether its value stayed the same or changed during 401.
        let later_resolved = source.resolve_explicitly();
        assert_eq!(source.read_count.get(), 2);
        assert_eq!(later_resolved.credential_origin, source.label);
        let expected_later_token = if mutate_source_during_401 {
            "changed-after-401"
        } else {
            "fixture-token"
        };
        assert_eq!(later_resolved.access_token(), expected_later_token);

        let later_fetches = Cell::new(0);
        let (_, later_rate_limit, later_error) = claude_resolved_view_with_fetch(
            "claude",
            Some("Anthropic / Claude"),
            NOW + 300,
            later_resolved,
            |token| {
                assert_eq!(token, expected_later_token);
                later_fetches.set(later_fetches.get() + 1);
                Ok(serde_json::from_value(serde_json::json!({}))
                    .expect("empty fake usage response decodes"))
            },
        );

        assert_eq!(later_fetches.get(), 1);
        assert_eq!(later_rate_limit, None);
        assert_eq!(later_error, None);
        assert_eq!(source.read_count.get(), 2);
    }
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
