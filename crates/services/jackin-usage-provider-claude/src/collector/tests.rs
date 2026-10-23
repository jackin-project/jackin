// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cell::{Cell, RefCell};

use super::*;
use crate::ClaudeCredentialBootstrapOutcome;
use crate::lease::{
    bootstrap_claude_credential_with_for_test, cached_claude_keychain_payload,
    clear_bootstrapped_claude_credential, overwrite_cached_generation_for_test,
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

fn resolved_for_test(token: &str) -> ClaudeResolved {
    ClaudeResolved::from_token(
        token.to_owned(),
        Some("Pro".to_owned()),
        Some("operator@example.test".to_owned()),
        Some("workspace".to_owned()),
        "OAuth · test".to_owned(),
        false,
    )
}

#[derive(Debug, PartialEq, Eq)]
struct ResolvedState {
    access_token: String,
    subscription_type: Option<String>,
    account_email: Option<String>,
    organization_type: Option<String>,
    credential_origin: String,
    is_anonymous: bool,
}

fn resolved_state(resolved: &ClaudeResolved) -> ResolvedState {
    ResolvedState {
        access_token: resolved.access_token().to_owned(),
        subscription_type: resolved.subscription_type.clone(),
        account_email: resolved.account_email.clone(),
        organization_type: resolved.organization_type.clone(),
        credential_origin: resolved.credential_origin.clone(),
        is_anonymous: resolved.is_anonymous,
    }
}

fn cached_payload_text(lease: &ClaudeCredentialLease) -> Option<String> {
    cached_payload_for_lease(lease).map(|payload| payload.as_str().to_owned())
}

fn is_consent_revoked_with_status(
    result: &Result<ClaudeOAuthUsageResponse, ClaudeFetchError>,
    expected_status: Option<u16>,
) -> bool {
    matches!(
        result,
        Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status,
        }) if *provider_http_status == expected_status
    )
}

fn is_provider_error_with_status(
    result: &Result<ClaudeOAuthUsageResponse, ClaudeFetchError>,
    expected_status: u16,
) -> bool {
    matches!(
        result,
        Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus {
            status,
            ..
        })) if *status == expected_status
    )
}

#[test]
fn c1_initial_consent_denial_skips_fetch_and_keychain_reread() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let mut resolved = resolved_for_test("old-token");
    let original_resolved = resolved_state(&resolved);
    let original_payload = cached_payload_text(&lease);
    let requests = Cell::new(0);
    let rereads = Cell::new(0);
    let consent_checks = Cell::new(0);
    let admissions = Cell::new(0);
    let commits = Cell::new(0);

    let result = fetch_claude_with_one_401_reread(
        &lease,
        &mut resolved,
        payload("old-token"),
        |_| {
            requests.set(requests.get() + 1);
            Err(http_error(401))
        },
        |_| {
            rereads.set(rereads.get() + 1);
            ClaudeKeychainRead::Payload {
                json: payload("new-token"),
            }
        },
        &mut || {
            consent_checks.set(consent_checks.get() + 1);
            false
        },
        || {
            admissions.set(admissions.get() + 1);
            true
        },
        |operation| {
            commits.set(commits.get() + 1);
            Some(operation())
        },
    );
    let unchanged = resolved_state(&resolved) == original_resolved
        && cached_payload_text(&lease) == original_payload;
    drop(lease);
    clear_bootstrapped_claude_credential();

    assert!(is_consent_revoked_with_status(&result, None));
    assert_eq!(requests.get(), 0);
    assert_eq!(rereads.get(), 0);
    assert_eq!(consent_checks.get(), 1);
    assert_eq!(admissions.get(), 0);
    assert_eq!(commits.get(), 0);
    assert!(
        unchanged,
        "initial denial preserves resolved state and cache"
    );
}

#[test]
fn c2_consent_revoked_after_first_401_skips_keychain_reread() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let mut resolved = resolved_for_test("old-token");
    let original_resolved = resolved_state(&resolved);
    let original_payload = cached_payload_text(&lease);
    let consent = Cell::new(true);
    let requests = Cell::new(0);
    let rereads = Cell::new(0);
    let admissions = Cell::new(0);
    let commits = Cell::new(0);

    let result = fetch_claude_with_one_401_reread(
        &lease,
        &mut resolved,
        payload("old-token"),
        |_| {
            requests.set(requests.get() + 1);
            consent.set(false);
            Err(http_error(401))
        },
        |_| {
            rereads.set(rereads.get() + 1);
            ClaudeKeychainRead::Payload {
                json: payload("new-token"),
            }
        },
        &mut || consent.get(),
        || {
            admissions.set(admissions.get() + 1);
            true
        },
        |operation| {
            commits.set(commits.get() + 1);
            Some(operation())
        },
    );
    let unchanged = resolved_state(&resolved) == original_resolved
        && cached_payload_text(&lease) == original_payload;
    drop(lease);
    clear_bootstrapped_claude_credential();

    assert!(is_consent_revoked_with_status(&result, Some(401)));
    assert_eq!(requests.get(), 1);
    assert_eq!(rereads.get(), 0);
    assert_eq!(admissions.get(), 1);
    assert_eq!(commits.get(), 0);
    assert!(
        unchanged,
        "revocation after 401 preserves resolved state and cache"
    );
}

#[test]
fn c3_consent_revoked_by_changed_keychain_reread_skips_retry() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let mut resolved = resolved_for_test("old-token");
    let original_resolved = resolved_state(&resolved);
    let original_payload = cached_payload_text(&lease);
    let consent = Cell::new(true);
    let requests = Cell::new(0);
    let rereads = Cell::new(0);
    let admissions = Cell::new(0);
    let commits = Cell::new(0);

    let result = fetch_claude_with_one_401_reread(
        &lease,
        &mut resolved,
        payload("old-token"),
        |_| {
            requests.set(requests.get() + 1);
            Err(http_error(401))
        },
        |_| {
            rereads.set(rereads.get() + 1);
            consent.set(false);
            ClaudeKeychainRead::Payload {
                json: payload("new-token"),
            }
        },
        &mut || consent.get(),
        || {
            admissions.set(admissions.get() + 1);
            true
        },
        |operation| {
            commits.set(commits.get() + 1);
            Some(operation())
        },
    );
    let unchanged = resolved_state(&resolved) == original_resolved
        && cached_payload_text(&lease) == original_payload;
    drop(lease);
    clear_bootstrapped_claude_credential();

    assert!(is_consent_revoked_with_status(&result, Some(401)));
    assert_eq!(requests.get(), 1);
    assert_eq!(rereads.get(), 1);
    assert_eq!(admissions.get(), 2);
    assert_eq!(commits.get(), 0);
    assert!(
        unchanged,
        "revocation after changed reread preserves resolved state and cache"
    );
}

#[derive(Debug)]
enum ConsentBoundaryDisposition {
    Revoked(Option<u16>),
    Other,
}

#[derive(Debug)]
struct ConsentBoundaryResult {
    disposition: ConsentBoundaryDisposition,
    resolved_unchanged: bool,
    request_tokens: Vec<String>,
    rereads: usize,
    reread_service: String,
    post_reread_checks: usize,
    consent_checks: usize,
    release_received: bool,
    admissions: usize,
    commits: usize,
}

fn spawn_consent_boundary_worker(
    lease: ClaudeCredentialLease,
    timeout: std::time::Duration,
    ready_sender: std::sync::mpsc::SyncSender<()>,
    release_receiver: std::sync::mpsc::Receiver<()>,
    done_sender: std::sync::mpsc::SyncSender<ConsentBoundaryResult>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut resolved = resolved_for_test("old-token");
        let original_resolved = resolved_state(&resolved);
        let request_tokens = RefCell::new(Vec::new());
        let rereads = Cell::new(0);
        let reread_service = RefCell::new(String::new());
        let reread_complete = Cell::new(false);
        let post_reread_checks = Cell::new(0);
        let consent_checks = Cell::new(0);
        let release_received = Cell::new(false);
        let admissions = Cell::new(0);
        let commits = Cell::new(0);

        let result = fetch_claude_with_one_401_reread(
            &lease,
            &mut resolved,
            payload("old-token"),
            |token| {
                request_tokens.borrow_mut().push(token.to_owned());
                if token == "old-token" {
                    Err(http_error(401))
                } else {
                    Ok(success_response())
                }
            },
            |selected_service| {
                rereads.set(rereads.get() + 1);
                *reread_service.borrow_mut() = selected_service.to_owned();
                reread_complete.set(true);
                ClaudeKeychainRead::Payload {
                    json: payload("changed-token"),
                }
            },
            &mut || {
                consent_checks.set(consent_checks.get() + 1);
                if !reread_complete.get() {
                    return true;
                }
                let checks = post_reread_checks.get() + 1;
                post_reread_checks.set(checks);
                if checks != 2 {
                    return true;
                }
                if ready_sender.send(()).is_err() {
                    return false;
                }
                release_received.set(release_receiver.recv_timeout(timeout).is_ok());
                false
            },
            || {
                admissions.set(admissions.get() + 1);
                true
            },
            |operation| {
                commits.set(commits.get() + 1);
                Some(operation())
            },
        );
        let disposition = match result {
            Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status,
            }) => ConsentBoundaryDisposition::Revoked(provider_http_status),
            Ok(_) | Err(ClaudeFetchError::Provider(_)) => ConsentBoundaryDisposition::Other,
        };
        let worker_result = ConsentBoundaryResult {
            disposition,
            resolved_unchanged: original_resolved == resolved_state(&resolved),
            request_tokens: request_tokens.into_inner(),
            rereads: rereads.get(),
            reread_service: reread_service.into_inner(),
            post_reread_checks: post_reread_checks.get(),
            consent_checks: consent_checks.get(),
            release_received: release_received.get(),
            admissions: admissions.get(),
            commits: commits.get(),
        };
        drop(lease);
        let _send_succeeded = done_sender.send(worker_result).is_ok();
    })
}

#[test]
fn c4_final_consent_denial_preserves_newer_global_generation_after_old_drop() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let old_generation = lease.generation();
    let timeout = std::time::Duration::from_secs(5);
    let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
    let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
    let (done_sender, done_receiver) = std::sync::mpsc::sync_channel(1);
    let worker = spawn_consent_boundary_worker(
        lease.clone(),
        timeout,
        ready_sender,
        release_receiver,
        done_sender,
    );

    let boundary_reached = ready_receiver.recv_timeout(timeout).is_ok();
    let newer_payload = payload("newer-session-token");
    let newer_payload_text = newer_payload.as_str().to_owned();
    let newer_generation =
        boundary_reached.then(|| overwrite_cached_generation_for_test(service, newer_payload));
    let release_sent = release_sender.send(()).is_ok();
    let worker_result = done_receiver.recv_timeout(timeout).ok();
    let worker_joined = worker.join().is_ok();
    drop(lease);
    let newer_payload_after_drop =
        cached_claude_keychain_payload(service).map(|payload| payload.as_str().to_owned());
    clear_bootstrapped_claude_credential();

    assert!(
        boundary_reached,
        "worker reaches the final consent boundary"
    );
    assert!(release_sent, "worker receives the bounded consent release");
    assert!(
        worker_joined,
        "worker joins before the final old lease drop"
    );
    let Some(result) = worker_result else {
        panic!("delayed collector worker reports its completed state")
    };
    assert!(matches!(
        result.disposition,
        ConsentBoundaryDisposition::Revoked(Some(401))
    ));
    assert!(result.resolved_unchanged);
    assert_eq!(result.request_tokens, ["old-token"]);
    assert_eq!(result.rereads, 1);
    assert_eq!(result.reread_service, service);
    assert_eq!(result.post_reread_checks, 2);
    assert_eq!(result.consent_checks, 4);
    assert!(result.release_received);
    assert_eq!(result.admissions, 2);
    assert_eq!(result.commits, 0);
    assert!(newer_generation.is_some_and(|generation| generation > old_generation));
    assert_eq!(newer_payload_after_drop, Some(newer_payload_text));
}
#[test]
fn c6_unchanged_keychain_reread_returns_original_401_without_retry() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let mut resolved = resolved_for_test("old-token");
    let original_resolved = resolved_state(&resolved);
    let original_payload = cached_payload_text(&lease);
    let requests = Cell::new(0);
    let rereads = Cell::new(0);
    let admissions = Cell::new(0);
    let commits = Cell::new(0);

    let result = fetch_claude_with_one_401_reread(
        &lease,
        &mut resolved,
        payload("old-token"),
        |_| {
            requests.set(requests.get() + 1);
            Err(http_error(401))
        },
        |_| {
            rereads.set(rereads.get() + 1);
            ClaudeKeychainRead::Payload {
                json: payload("old-token"),
            }
        },
        &mut admit_current,
        || {
            admissions.set(admissions.get() + 1);
            true
        },
        |operation| {
            commits.set(commits.get() + 1);
            Some(operation())
        },
    );
    let unchanged = resolved_state(&resolved) == original_resolved
        && cached_payload_text(&lease) == original_payload;
    drop(lease);
    clear_bootstrapped_claude_credential();

    assert!(is_provider_error_with_status(&result, 401));
    assert_eq!(requests.get(), 1);
    assert_eq!(rereads.get(), 1);
    assert_eq!(admissions.get(), 2);
    assert_eq!(commits.get(), 0);
    assert!(
        unchanged,
        "unchanged reread preserves resolved state and cache"
    );
}

#[test]
fn operation_admission_denied_after_changed_reread_skips_retry_and_commit() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let mut resolved = resolved_for_test("old-token");
    let original_resolved = resolved_state(&resolved);
    let original_payload = cached_payload_text(&lease);
    let requests = Cell::new(0);
    let rereads = Cell::new(0);
    let consent_checks = Cell::new(0);
    let admissions = Cell::new(0);
    let commits = Cell::new(0);

    let result = fetch_claude_with_one_401_reread(
        &lease,
        &mut resolved,
        payload("old-token"),
        |token| {
            requests.set(requests.get() + 1);
            if token == "old-token" {
                Err(http_error(401))
            } else {
                Ok(success_response())
            }
        },
        |_| {
            rereads.set(rereads.get() + 1);
            ClaudeKeychainRead::Payload {
                json: payload("new-token"),
            }
        },
        &mut || {
            consent_checks.set(consent_checks.get() + 1);
            true
        },
        || {
            admissions.set(admissions.get() + 1);
            admissions.get() < 3
        },
        |operation| {
            commits.set(commits.get() + 1);
            Some(operation())
        },
    );
    let unchanged = resolved_state(&resolved) == original_resolved
        && cached_payload_text(&lease) == original_payload;
    drop(lease);
    clear_bootstrapped_claude_credential();

    assert!(is_consent_revoked_with_status(&result, Some(401)));
    assert_eq!(requests.get(), 1);
    assert_eq!(rereads.get(), 1);
    assert_eq!(consent_checks.get(), 4);
    assert_eq!(
        admissions.get(),
        3,
        "third admission is denied after reread"
    );
    assert_eq!(commits.get(), 0);
    assert!(
        unchanged,
        "admission denial preserves resolved state and cache"
    );
}

#[test]
fn changed_exact_source_is_reread_and_retried_once_after_401() {
    let _serial = serialized_credential_cache_test();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let lease = lease(service, "old-token");
    let tokens = RefCell::new(Vec::new());
    let rereads = Cell::new(0);
    let admissions = Cell::new(0);

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
        || {
            admissions.set(admissions.get() + 1);
            true
        },
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
    assert_eq!(admissions.get(), 3);
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
