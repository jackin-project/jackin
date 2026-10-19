// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cell::{Cell, RefCell};

use super::*;
use crate::ClaudeCredentialBootstrapOutcome;
use crate::lease::{
    bootstrap_claude_credential_with_for_test, clear_bootstrapped_claude_credential,
    serialized_credential_cache_test,
};

fn payload(token: &str) -> Zeroizing<String> {
    Zeroizing::new(format!(
        r#"{{"claudeAiOauth":{{"accessToken":"{token}"}},"oauthAccount":{{"emailAddress":"operator@example.test"}}}}"#
    ))
}

fn lease(service: &str, token: &str) -> ClaudeCredentialLease {
    let outcome =
        bootstrap_claude_credential_with_for_test(service, true, || ClaudeKeychainRead::Payload {
            json: payload(token),
        })
        .expect("valid test credential should bootstrap");
    let ClaudeCredentialBootstrapOutcome::Acquired(lease) = outcome else {
        panic!("valid test credential should acquire a lease")
    };
    lease
}

fn success_response() -> ClaudeOAuthUsageResponse {
    serde_json::from_str(r#"{"five_hour":{"utilization":24}}"#)
        .expect("minimal Claude usage response")
}

fn http_error(status: u16) -> ProviderHttpError {
    ProviderHttpError::HttpStatus {
        status,
        message: format!("Claude usage HTTP {status}"),
        retry_after_seconds: (status == 429).then_some(60),
        response_received_at_epoch: Some(1_800_000_000),
    }
}

fn admit_current() -> bool {
    true
}

fn commit_current(operation: &mut dyn FnMut() -> bool) -> Option<bool> {
    Some(operation())
}

#[test]
fn changed_exact_source_is_reread_and_retried_once_after_401() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let tokens = RefCell::new(Vec::new());
    let rereads = Cell::new(0);

    let result = experimental_claude_usage_snapshot_for_lease_with(
        "claude",
        Some("Claude"),
        1_800_000_000,
        &lease,
        |token| {
            tokens.borrow_mut().push(token.to_owned());
            if token == "old-token" {
                Err(http_error(401))
            } else {
                Ok(success_response())
            }
        },
        |selected_service| {
            assert_eq!(selected_service, service);
            rereads.set(rereads.get() + 1);
            ClaudeKeychainRead::Payload {
                json: payload("new-token"),
            }
        },
        || true,
        admit_current,
        commit_current,
    )
    .expect("consent remains active");

    let Some((view, _, _)) = result else {
        panic!("exact changed credential should produce a provider view")
    };
    assert_eq!(
        view.status,
        jackin_protocol::control::UsageSnapshotStatus::Fresh
    );
    assert_eq!(tokens.borrow().as_slice(), ["old-token", "new-token"]);
    assert_eq!(rereads.get(), 1);
    let expected_payload = payload("new-token");
    assert_eq!(
        cached_payload_for_lease(&lease)
            .as_deref()
            .map(String::as_str),
        Some(expected_payload.as_str())
    );
    let serialized = serde_json::to_string(&view).unwrap();
    assert!(!serialized.contains("old-token"));
    assert!(!serialized.contains("new-token"));
    drop(lease);
    clear_bootstrapped_claude_credential();
}

#[test]
fn consent_revoked_after_success_discards_the_provider_view() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let lease = lease("Claude Code-credentials-selected", "selected-token");
    let consent_checks = Cell::new(0);
    let requests = Cell::new(0);
    let result = experimental_claude_usage_snapshot_for_lease_with(
        "claude",
        Some("Claude"),
        1_800_000_000,
        &lease,
        |_| {
            requests.set(requests.get() + 1);
            Ok(success_response())
        },
        |_| panic!("successful request must not reread Keychain"),
        || {
            consent_checks.set(consent_checks.get() + 1);
            consent_checks.get() < 4
        },
        admit_current,
        commit_current,
    );

    assert_eq!(requests.get(), 1);
    assert_eq!(consent_checks.get(), 4);
    assert!(matches!(
        result,
        Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status: None,
        })
    ));
    drop(lease);
    clear_bootstrapped_claude_credential();
}

#[test]
fn consent_revoked_after_provider_error_discards_error_and_keeps_only_status() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let lease = lease("Claude Code-credentials-selected", "selected-token");
    let consent = Cell::new(true);
    let requests = Cell::new(0);
    let result = experimental_claude_usage_snapshot_for_lease_with(
        "claude",
        Some("Claude"),
        1_800_000_000,
        &lease,
        |_| {
            requests.set(requests.get() + 1);
            consent.set(false);
            Err(http_error(429))
        },
        |_| panic!("a non-401 response must not reread Keychain"),
        || consent.get(),
        admit_current,
        commit_current,
    );

    assert_eq!(requests.get(), 1);
    assert!(matches!(
        result,
        Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status: Some(429),
        })
    ));
    drop(lease);
    clear_bootstrapped_claude_credential();
}

#[test]
fn consent_revoked_after_missing_keychain_reread_discards_first_401_view() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let lease = lease("Claude Code-credentials-selected", "selected-token");
    let consent = Cell::new(true);
    let requests = Cell::new(0);
    let result = experimental_claude_usage_snapshot_for_lease_with(
        "claude",
        Some("Claude"),
        1_800_000_000,
        &lease,
        |_| {
            requests.set(requests.get() + 1);
            Err(http_error(401))
        },
        |_| {
            consent.set(false);
            ClaudeKeychainRead::Missing
        },
        || consent.get(),
        admit_current,
        commit_current,
    );

    assert_eq!(requests.get(), 1);
    assert!(matches!(
        result,
        Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status: Some(401),
        })
    ));
    drop(lease);
    clear_bootstrapped_claude_credential();
}

#[test]
fn consent_revoked_after_retry_discards_provider_error_and_keeps_only_status() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let consent = Cell::new(true);
    let requests = Cell::new(0);
    let rereads = Cell::new(0);
    let result = experimental_claude_usage_snapshot_for_lease_with(
        "claude",
        Some("Claude"),
        1_800_000_000,
        &lease,
        |token| {
            requests.set(requests.get() + 1);
            if token == "old-token" {
                Err(http_error(401))
            } else {
                consent.set(false);
                Err(http_error(429))
            }
        },
        |selected_service| {
            assert_eq!(selected_service, service);
            rereads.set(rereads.get() + 1);
            ClaudeKeychainRead::Payload {
                json: payload("new-token"),
            }
        },
        || consent.get(),
        admit_current,
        commit_current,
    );

    assert_eq!(requests.get(), 2);
    assert_eq!(rereads.get(), 1);
    assert!(matches!(
        result,
        Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status: Some(429),
        })
    ));
    drop(lease);
    clear_bootstrapped_claude_credential();
}

#[test]
fn forbidden_and_rate_limited_responses_never_trigger_keychain_reread() {
    let _serial = serialized_credential_cache_test();
    for status in [403, 429] {
        clear_bootstrapped_claude_credential();
        let lease = lease("Claude Code-credentials-selected", "selected-token");
        let requests = Cell::new(0);
        let result = experimental_claude_usage_snapshot_for_lease_with(
            "claude",
            Some("Claude"),
            1_800_000_000,
            &lease,
            |_| {
                requests.set(requests.get() + 1);
                Err(http_error(status))
            },
            |_| panic!("only a typed HTTP 401 may reread Keychain"),
            || true,
            admit_current,
            commit_current,
        )
        .expect("consent remains active");

        let Some((_, rate_limit, failure)) = result else {
            panic!("provider error should produce a safe view")
        };
        assert_eq!(requests.get(), 1);
        assert_eq!(
            failure.map(|failure| failure.http_status),
            Some(Some(status))
        );
        assert_eq!(rate_limit.is_some(), status == 429);
        drop(lease);
    }
    clear_bootstrapped_claude_credential();
}
