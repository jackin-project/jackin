// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Consent-fenced Claude OAuth collection for one foreground source lease.

use zeroize::Zeroizing;

use jackin_protocol::control::FocusedUsageView;
use jackin_usage_provider_core::{ProviderFailureMetadata, ProviderHttpError, ProviderRateLimit};

use crate::keychain::{ClaudeKeychainRead, read_claude_keychain_item_uncached};

use super::{
    ClaudeCredentialLease, ClaudeOAuthUsageResponse, ClaudeResolved,
    credentials::parse_claude_keychain_profile,
    lease::{begin_unauthorized_reread, cached_payload_for_lease, valid_claude_keychain_service},
    spend::fetch_claude_oauth_usage,
    wave::claude_resolved_view_from_result,
};
#[cfg(any(target_os = "macos", test))]
use super::lease::{MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES, replace_if_exact};

/// A consent race can suppress collection while retaining an observed HTTP
/// failure status for safe broker diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeCollectionError {
    ConsentRevoked { provider_http_status: Option<u16> },
}

/// Collect from the exact source captured by an active foreground lease.
/// Every network and Keychain boundary rechecks current monitor consent.
pub fn experimental_claude_usage_snapshot_for_lease<C>(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    lease: &ClaudeCredentialLease,
    consent_is_current: C,
) -> Result<
    Option<(
        FocusedUsageView,
        Option<ProviderRateLimit>,
        Option<ProviderFailureMetadata>,
    )>,
    ClaudeCollectionError,
>
where
    C: FnMut() -> bool,
{
    experimental_claude_usage_snapshot_for_lease_with(
        agent,
        provider,
        now,
        lease,
        fetch_claude_oauth_usage,
        read_claude_keychain_item_uncached,
        consent_is_current,
    )
}

fn experimental_claude_usage_snapshot_for_lease_with<F, R, C>(
    agent: &str,
    provider: Option<&str>,
    now: i64,
    lease: &ClaudeCredentialLease,
    fetch: F,
    reread: R,
    mut consent_is_current: C,
) -> Result<
    Option<(
        FocusedUsageView,
        Option<ProviderRateLimit>,
        Option<ProviderFailureMetadata>,
    )>,
    ClaudeCollectionError,
>
where
    F: FnMut(&str) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError>,
    R: FnOnce(&str) -> ClaudeKeychainRead,
    C: FnMut() -> bool,
{
    if !consent_is_current() {
        return Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status: None,
        });
    }
    if !valid_claude_keychain_service(lease.service()) {
        return Ok(None);
    }
    let Some(payload) = cached_payload_for_lease(lease) else {
        return Ok(None);
    };
    let Some(profile) = parse_claude_keychain_profile(payload.as_bytes()) else {
        return Ok(None);
    };
    let Some(credential) = profile.credential else {
        return Ok(None);
    };
    let mut resolved = ClaudeResolved::from_oauth_credentials(
        credential,
        profile.account_email,
        profile.organization_type,
        "OAuth · macOS Keychain".to_owned(),
        false,
    );
    let result = fetch_claude_with_one_401_reread(
        lease,
        &mut resolved,
        payload,
        fetch,
        reread,
        &mut consent_is_current,
    );
    let result = match result {
        Ok(response) => Ok(response),
        Err(ClaudeFetchError::Provider(error)) => Err(error),
        Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status,
        }) => {
            return Err(ClaudeCollectionError::ConsentRevoked {
                provider_http_status,
            });
        }
    };
    let provider_http_status = match &result {
        Err(ProviderHttpError::HttpStatus { status, .. }) => Some(*status),
        Ok(_) | Err(_) => None,
    };
    let view = claude_resolved_view_from_result(agent, provider, now, resolved, result);
    if !consent_is_current() {
        return Err(ClaudeCollectionError::ConsentRevoked {
            provider_http_status,
        });
    }
    Ok(Some(view))
}

#[derive(Debug)]
enum ClaudeFetchError {
    Provider(ProviderHttpError),
    ConsentRevoked { provider_http_status: Option<u16> },
}

fn fetch_claude_with_one_401_reread<F, R, C>(
    lease: &ClaudeCredentialLease,
    resolved: &mut ClaudeResolved,
    #[cfg(any(target_os = "macos", test))] original_payload: Zeroizing<String>,
    #[cfg(not(any(target_os = "macos", test)))] _original_payload: Zeroizing<String>,
    mut fetch: F,
    reread: R,
    consent_is_current: &mut C,
) -> Result<ClaudeOAuthUsageResponse, ClaudeFetchError>
where
    F: FnMut(&str) -> Result<ClaudeOAuthUsageResponse, ProviderHttpError>,
    R: FnOnce(&str) -> ClaudeKeychainRead,
    C: FnMut() -> bool,
{
    if !consent_is_current() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: None,
        });
    }
    let first = fetch(resolved.access_token()).map_err(ClaudeFetchError::Provider);
    if !matches!(
        &first,
        Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus {
            status: 401,
            ..
        }))
    ) {
        if !consent_is_current() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: provider_http_status(&first),
            });
        }
        return first;
    }
    if !consent_is_current() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    }
    if !begin_unauthorized_reread(lease) {
        return first_401_if_consent_current(first, consent_is_current);
    }
    let keychain_read = reread(lease.service());
    if !consent_is_current() {
        return Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        });
    }
    #[cfg(any(target_os = "macos", test))]
    let Some(json) = payload_from_keychain_read(keychain_read) else {
        return first_401_if_consent_current(first, consent_is_current);
    };
    #[cfg(not(any(target_os = "macos", test)))]
    {
        drop(keychain_read);
        return first_401_if_consent_current(first, consent_is_current);
    }
    #[cfg(any(target_os = "macos", test))]
    {
        if json.len() > MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES {
            return first_401_if_consent_current(first, consent_is_current);
        }
        let Some(profile) = parse_claude_keychain_profile(json.as_bytes()) else {
            return first_401_if_consent_current(first, consent_is_current);
        };
        let Some(credential) = profile.credential else {
            return first_401_if_consent_current(first, consent_is_current);
        };
        if resolved
            .account_email
            .as_deref()
            .zip(profile.account_email.as_deref())
            .is_some_and(|(old, new)| old != new)
            || credential.access_token.as_str() == resolved.access_token()
        {
            return first_401_if_consent_current(first, consent_is_current);
        }
        if !consent_is_current() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: Some(401),
            });
        }
        let access_token = credential.access_token;
        resolved.replace_access_token(access_token.clone());
        let retried = fetch(resolved.access_token()).map_err(ClaudeFetchError::Provider);
        if !consent_is_current() {
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: provider_http_status(&retried),
            });
        }
        let cache_updated = replace_if_exact(lease, json);
        if !consent_is_current() {
            if cache_updated {
                let _restored = replace_if_exact(lease, original_payload);
            }
            return Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: provider_http_status(&retried),
            });
        }
        if !cache_updated {
            return retried;
        }
        Ok(retried?)
    }
}

fn provider_http_status(
    result: &Result<ClaudeOAuthUsageResponse, ClaudeFetchError>,
) -> Option<u16> {
    match result {
        Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus { status, .. })) => {
            Some(*status)
        }
        Ok(_)
        | Err(ClaudeFetchError::Provider(_))
        | Err(ClaudeFetchError::ConsentRevoked { .. }) => None,
    }
}

fn first_401_if_consent_current<C>(
    first: Result<ClaudeOAuthUsageResponse, ClaudeFetchError>,
    consent_is_current: &mut C,
) -> Result<ClaudeOAuthUsageResponse, ClaudeFetchError>
where
    C: FnMut() -> bool,
{
    if consent_is_current() {
        first
    } else {
        Err(ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401),
        })
    }
}

#[cfg(any(target_os = "macos", test))]
fn payload_from_keychain_read(read: ClaudeKeychainRead) -> Option<Zeroizing<String>> {
    match read {
        ClaudeKeychainRead::Payload { json } => Some(json),
        ClaudeKeychainRead::Denied
        | ClaudeKeychainRead::Missing
        | ClaudeKeychainRead::ConsentRequired => None,
    }
}

#[cfg(test)]
mod tests {
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

    fn resolved(token: &str) -> ClaudeResolved {
        ClaudeResolved::from_token(
            token.to_owned(),
            None,
            Some("operator@example.test".to_owned()),
            None,
            "OAuth · macOS Keychain".to_owned(),
            false,
        )
    }

    fn unauthorized(status: u16) -> ProviderHttpError {
        ProviderHttpError::HttpStatus {
            status,
            message: format!("Claude usage HTTP {status}"),
            retry_after_seconds: None,
            response_received_at_epoch: None,
        }
    }

    fn lease(service: &str, token: &str) -> ClaudeCredentialLease {
        let outcome = bootstrap_claude_credential_with_for_test(service, true, || {
            ClaudeKeychainRead::Payload {
                json: payload(token),
            }
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

    #[test]
    fn revoked_consent_before_first_request_skips_provider_and_keychain() {
        let _serial = serialized_credential_cache_test();
        clear_bootstrapped_claude_credential();
        let service = "Claude Code-credentials-selected";
        let lease = lease(service, "old-token");
        let mut resolved = resolved("old-token");
        let fetch_count = Cell::new(0);
        let reread_count = Cell::new(0);
        let result = fetch_claude_with_one_401_reread(
            &lease,
            &mut resolved,
            payload("old-token"),
            |_| {
                fetch_count.set(fetch_count.get() + 1);
                Err(unauthorized(401))
            },
            |_| {
                reread_count.set(reread_count.get() + 1);
                ClaudeKeychainRead::Missing
            },
            &mut || false,
        );

        assert!(matches!(
            result,
            Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: None
            })
        ));
        assert_eq!(fetch_count.get(), 0);
        assert_eq!(reread_count.get(), 0);
        drop(lease);
        clear_bootstrapped_claude_credential();
    }

    #[test]
    fn revoked_consent_after_401_skips_keychain_reread_and_preserves_401() {
        let _serial = serialized_credential_cache_test();
        clear_bootstrapped_claude_credential();
        let service = "Claude Code-credentials-selected";
        let lease = lease(service, "old-token");
        let mut resolved = resolved("old-token");
        let fetch_count = Cell::new(0);
        let reread_count = Cell::new(0);
        let validation_count = Cell::new(0);
        let result = fetch_claude_with_one_401_reread(
            &lease,
            &mut resolved,
            payload("old-token"),
            |_| {
                fetch_count.set(fetch_count.get() + 1);
                Err(unauthorized(401))
            },
            |_| {
                reread_count.set(reread_count.get() + 1);
                ClaudeKeychainRead::Payload {
                    json: payload("new-token"),
                }
            },
            &mut || {
                let call = validation_count.get();
                validation_count.set(call + 1);
                call == 0
            },
        );

        assert!(matches!(
            result,
            Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: Some(401)
            })
        ));
        assert_eq!(fetch_count.get(), 1);
        assert_eq!(reread_count.get(), 0);
        assert_eq!(validation_count.get(), 2);
        assert_eq!(
            cached_payload_for_lease(&lease)
                .as_deref()
                .map(String::as_str),
            Some(payload("old-token").as_str())
        );
        drop(lease);
        clear_bootstrapped_claude_credential();
    }

    #[test]
    fn revoked_consent_after_keychain_reread_skips_retry_and_preserves_401() {
        let _serial = serialized_credential_cache_test();
        clear_bootstrapped_claude_credential();
        let service = "Claude Code-credentials-selected";
        let lease = lease(service, "old-token");
        let mut resolved = resolved("old-token");
        let fetch_count = Cell::new(0);
        let reread_count = Cell::new(0);
        let revoked = Cell::new(false);
        let result = fetch_claude_with_one_401_reread(
            &lease,
            &mut resolved,
            payload("old-token"),
            |_| {
                fetch_count.set(fetch_count.get() + 1);
                Err(unauthorized(401))
            },
            |_| {
                reread_count.set(reread_count.get() + 1);
                revoked.set(true);
                ClaudeKeychainRead::Payload {
                    json: payload("new-token"),
                }
            },
            &mut || !revoked.get(),
        );

        assert!(matches!(
            result,
            Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: Some(401)
            })
        ));
        assert_eq!(fetch_count.get(), 1);
        assert_eq!(reread_count.get(), 1);
        assert_eq!(resolved.access_token(), "old-token");
        assert_eq!(
            cached_payload_for_lease(&lease)
                .as_deref()
                .map(String::as_str),
            Some(payload("old-token").as_str())
        );
        drop(lease);
        clear_bootstrapped_claude_credential();
    }

    #[test]
    fn stop_before_retry_admission_skips_fetch_after_changed_credential_reread() {
        let _serial = serialized_credential_cache_test();
        clear_bootstrapped_claude_credential();
        let service = "Claude Code-credentials-selected";
        let lease = lease(service, "old-token");
        let fetch_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reread_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (retry_admission_tx, retry_admission_rx) = std::sync::mpsc::channel();
        let (retry_admission_release_tx, retry_admission_release_rx) = std::sync::mpsc::channel();
        let task_fetch_count = std::sync::Arc::clone(&fetch_count);
        let task_reread_count = std::sync::Arc::clone(&reread_count);
        let consent_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let task_consent_calls = std::sync::Arc::clone(&consent_calls);

        let task = std::thread::spawn(move || {
            let mut resolved = resolved("old-token");
            fetch_claude_with_one_401_reread(
                &lease,
                &mut resolved,
                payload("old-token"),
                move |_| {
                    task_fetch_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Err(unauthorized(401))
                },
                move |selected_service| {
                    assert_eq!(selected_service, service);
                    task_reread_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ClaudeKeychainRead::Payload {
                        json: payload("new-token"),
                    }
                },
                &mut move || {
                    let attempt = task_consent_calls
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if attempt == 3 {
                        retry_admission_tx
                            .send(())
                            .expect("signal before retry admission");
                        retry_admission_release_rx
                            .recv()
                            .expect("release retry admission after stop");
                        return false;
                    }
                    true
                },
            )
        });

        retry_admission_rx
            .recv()
            .expect("changed credential reaches the retry admission gate");
        crate::lease::overwrite_cached_generation_for_test(
            service,
            payload("newer-session-token"),
        );
        retry_admission_release_tx
            .send(())
            .expect("retry admission observes deactivation");

        assert!(matches!(
            task.join()
                .expect("collector exits after denied admission"),
            Err(ClaudeFetchError::ConsentRevoked {
                provider_http_status: Some(401)
            })
        ));
        assert_eq!(
            fetch_count.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        assert_eq!(
            reread_count.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let expected = payload("newer-session-token");
        assert_eq!(
            crate::lease::cached_claude_keychain_payload(service)
                .as_deref()
                .map(String::as_str),
            Some(expected.as_str())
        );
        clear_bootstrapped_claude_credential();
    }

    #[test]
    fn typed_403_and_unchanged_401_never_trigger_a_second_fetch() {
        let _serial = serialized_credential_cache_test();
        clear_bootstrapped_claude_credential();
        let service = "Claude Code-credentials-selected";
        let lease = lease(service, "same-token");
        let mut resolved = resolved("same-token");
        let fetch_count = Cell::new(0);
        let reread_count = Cell::new(0);
        let result = fetch_claude_with_one_401_reread(
            &lease,
            &mut resolved,
            payload("same-token"),
            |_| {
                fetch_count.set(fetch_count.get() + 1);
                Err(unauthorized(403))
            },
            |_| {
                reread_count.set(reread_count.get() + 1);
                ClaudeKeychainRead::Payload {
                    json: payload("same-token"),
                }
            },
            &mut || true,
        );
        assert!(matches!(
            result,
            Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus {
                status: 403,
                ..
            }))
        ));
        assert_eq!(fetch_count.get(), 1);
        assert_eq!(reread_count.get(), 0);

        let result = fetch_claude_with_one_401_reread(
            &lease,
            &mut resolved,
            payload("same-token"),
            |_| {
                fetch_count.set(fetch_count.get() + 1);
                Err(unauthorized(401))
            },
            |_| {
                reread_count.set(reread_count.get() + 1);
                ClaudeKeychainRead::Payload {
                    json: payload("same-token"),
                }
            },
            &mut || true,
        );
        assert!(matches!(
            result,
            Err(ClaudeFetchError::Provider(ProviderHttpError::HttpStatus {
                status: 401,
                ..
            }))
        ));
        assert_eq!(fetch_count.get(), 2);
        assert_eq!(reread_count.get(), 1);
        assert_eq!(resolved.access_token(), "same-token");
        drop(lease);
        clear_bootstrapped_claude_credential();
    }
}
