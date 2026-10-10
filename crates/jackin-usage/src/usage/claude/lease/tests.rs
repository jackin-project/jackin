// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::cell::{Cell, RefCell};
use std::sync::Mutex;

fn test_lease_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[test]
fn cache_is_bounded_to_one_exact_service_and_zeroizing_payload() {
    let cache = ClaudeCredentialCache::default();
    *cache
        .credential
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(CachedClaudeCredential {
        service: "Claude Code-credentials-a".to_owned(),
        json: Zeroizing::new("selected-token".to_owned()),
        generation: 5,
        unauthorized_reread_attempted: false,
    });

    let selected = cache
        .payload("Claude Code-credentials-a")
        .expect("exact service cache hit");
    assert!(selected.as_str() == "selected-token");
    assert!(cache.payload("Claude Code-credentials-b").is_none());
    assert_eq!(
        cache.begin_unauthorized_reread("Claude Code-credentials-a"),
        Some(5)
    );
    assert!(cache.replace_if_exact(
        "Claude Code-credentials-a",
        5,
        Zeroizing::new("rotated-token".to_owned())
    ));
    assert_eq!(
        cache.begin_unauthorized_reread("Claude Code-credentials-a"),
        None
    );
    assert!(!cache.replace_if_exact(
        "Claude Code-credentials-b",
        5,
        Zeroizing::new("other-token".to_owned())
    ));
    assert!(
        cache
            .payload("Claude Code-credentials-a")
            .is_some_and(|payload| payload.as_str() == "rotated-token")
    );
    assert!(
        cache
            .service()
            .is_some_and(|service| service == "Claude Code-credentials-a")
    );

    cache.clear();
    assert!(cache.service().is_none());
}

#[test]
fn service_identity_hash_preserves_exact_selected_bytes() {
    let exact_service = "Claude Code-credentials-91a2bc34";
    let expected = jackin_core::account_key_hash("claude-keychain-service-v1", exact_service);
    assert_eq!(
        super::super::claude_source_capability_id_for_service(exact_service),
        expected.strip_prefix("sha256:").unwrap_or(&expected)
    );
    assert_ne!(
        super::super::claude_source_capability_id_for_service(exact_service),
        super::super::claude_source_capability_id_for_service(" Claude Code-credentials-91a2bc34")
    );

    assert!(!valid_claude_keychain_service("   "));
    assert!(!valid_claude_keychain_service("Claude\0Code"));
    assert!(valid_claude_keychain_service(exact_service));
    let oversized_service = "x".repeat(MAX_CLAUDE_KEYCHAIN_SERVICE_BYTES + 1);
    assert!(!valid_claude_keychain_service(&oversized_service));
}

#[test]
fn bootstrap_payload_validation_rejects_oversized_and_noncredential_json() {
    let valid = r#"{"claudeAiOauth":{"accessToken":"selected-token"}}"#;
    assert!(valid_claude_keychain_payload(valid));
    assert!(!valid_claude_keychain_payload(r#"{"claudeAiOauth":{}}"#));
    assert!(!valid_claude_keychain_payload("not-json"));
    let oversized_payload = format!("{valid}{}", " ".repeat(MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES));
    assert!(!valid_claude_keychain_payload(&oversized_payload));
}

#[test]
fn dropping_a_lease_clears_only_its_cache_generation() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let old_generation = next_generation();
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(r#"{"claudeAiOauth":{"accessToken":"old"}}"#.to_owned()),
        old_generation,
    );
    let old_lease = ClaudeCredentialLease {
        service: service.to_owned(),
        generation: old_generation,
    };

    let current_generation = next_generation();
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(r#"{"claudeAiOauth":{"accessToken":"current"}}"#.to_owned()),
        current_generation,
    );
    drop(old_lease);
    assert!(
        cached_credential()
            .payload(service)
            .is_some_and(|payload| payload.contains("current"))
    );

    drop(ClaudeCredentialLease {
        service: service.to_owned(),
        generation: current_generation,
    });
    assert!(cached_credential().service().is_none());
}

fn resolved_from_payload(json: &str, service: &str) -> super::super::ClaudeResolved {
    let profile = super::super::parse_claude_profile_payload(json.as_bytes()).expect("payload");
    let credential = profile.credential.expect("OAuth credential");
    super::super::ClaudeResolved {
        access_token: credential.access_token,
        subscription_type: credential.subscription_type,
        account_email: profile.account_email,
        organization_type: profile.organization_type,
        credential_origin: format!("OAuth · macOS Keychain ({service})"),
        keychain_service: Some(service.to_owned()),
        is_anonymous: false,
    }
}

#[test]
fn typed_401_allows_one_changed_exact_source_retry() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"},"oauthAccount":{"emailAddress":"same@example.test"}}"#;
    let replacement = r#"{"claudeAiOauth":{"accessToken":"new-token"},"oauthAccount":{"emailAddress":"same@example.test"}}"#;
    let generation = next_generation();
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        generation,
    );

    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let seen_tokens = RefCell::new(Vec::new());
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |token| {
            seen_tokens.borrow_mut().push(token.to_owned());
            if fetch_count.replace(fetch_count.get() + 1) == 0 {
                Err(crate::usage::ProviderHttpError::HttpStatus {
                    status: 401,
                    message: "unauthorized".to_owned(),
                    retry_after_seconds: None,
                    response_received_at_epoch: None,
                })
            } else {
                serde_json::from_str::<crate::usage::ClaudeOAuthUsageResponse>("{}")
                    .map_err(|error| crate::usage::ProviderHttpError::Decode(error.to_string()))
            }
        },
        |requested_service| {
            assert_eq!(requested_service, service);
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Payload {
                json: Zeroizing::new(replacement.to_owned()),
            }
        },
        || true,
    );

    assert!(result.is_ok());
    assert_eq!(fetch_count.get(), 2);
    assert_eq!(reread_count.get(), 1);
    assert_eq!(&*seen_tokens.borrow(), &["old-token", "new-token"]);
    assert_eq!(resolved.access_token.as_str(), "new-token");
    assert!(
        cached_credential()
            .payload(service)
            .is_some_and(|payload| payload.as_str() == replacement)
    );
    clear_bootstrapped_claude_credential();
}

#[test]
fn revoked_consent_before_first_request_skips_provider_and_keychain() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        next_generation(),
    );
    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            fetch_count.set(fetch_count.get() + 1);
            Err(crate::usage::ProviderHttpError::HttpStatus {
                status: 401,
                message: "unauthorized".to_owned(),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            })
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Missing
        },
        || false,
    );

    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::ConsentRevoked {
            provider_http_status: None
        })
    ));
    assert_eq!(fetch_count.get(), 0);
    assert_eq!(reread_count.get(), 0);
    clear_bootstrapped_claude_credential();
}

#[test]
fn revoked_consent_after_first_success_rejects_provider_result() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        next_generation(),
    );
    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let revoked = Cell::new(false);
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            fetch_count.set(fetch_count.get() + 1);
            revoked.set(true);
            serde_json::from_str::<crate::usage::ClaudeOAuthUsageResponse>("{}")
                .map_err(|error| crate::usage::ProviderHttpError::Decode(error.to_string()))
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Missing
        },
        || !revoked.get(),
    );

    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::ConsentRevoked {
            provider_http_status: None
        })
    ));
    assert_eq!(fetch_count.get(), 1);
    assert_eq!(reread_count.get(), 0);
    clear_bootstrapped_claude_credential();
}

#[test]
fn first_provider_error_is_preserved_when_consent_revokes_after_response() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        next_generation(),
    );
    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let revoked = Cell::new(false);
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            fetch_count.set(fetch_count.get() + 1);
            revoked.set(true);
            Err(crate::usage::ProviderHttpError::HttpStatus {
                status: 429,
                message: "rate limited".to_owned(),
                retry_after_seconds: Some(120),
                response_received_at_epoch: Some(1_000),
            })
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Missing
        },
        || !revoked.get(),
    );

    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::Provider(
            crate::usage::ProviderHttpError::HttpStatus {
                status: 429,
                retry_after_seconds: Some(120),
                response_received_at_epoch: Some(1_000),
                ..
            }
        ))
    ));
    assert_eq!(fetch_count.get(), 1);
    assert_eq!(reread_count.get(), 0);
    clear_bootstrapped_claude_credential();
}

#[test]
fn revoked_consent_after_401_skips_keychain_reread_and_preserves_401() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    let replacement = r#"{"claudeAiOauth":{"accessToken":"new-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        next_generation(),
    );
    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let validation_count = Cell::new(0);
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            fetch_count.set(fetch_count.get() + 1);
            Err(crate::usage::ProviderHttpError::HttpStatus {
                status: 401,
                message: "unauthorized".to_owned(),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            })
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Payload {
                json: Zeroizing::new(replacement.to_owned()),
            }
        },
        || {
            let call = validation_count.get();
            validation_count.set(call + 1);
            call == 0
        },
    );

    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401)
        })
    ));
    assert_eq!(fetch_count.get(), 1);
    assert_eq!(reread_count.get(), 0);
    assert_eq!(validation_count.get(), 2);
    assert!(
        cached_credential()
            .payload(service)
            .is_some_and(|payload| payload.as_str() == original)
    );
    clear_bootstrapped_claude_credential();
}

#[test]
fn revoked_consent_after_keychain_reread_skips_retry_and_preserves_401() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    let replacement = r#"{"claudeAiOauth":{"accessToken":"new-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        next_generation(),
    );
    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let revoked = Cell::new(false);
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            fetch_count.set(fetch_count.get() + 1);
            Err(crate::usage::ProviderHttpError::HttpStatus {
                status: 401,
                message: "unauthorized".to_owned(),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            })
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            revoked.set(true);
            ClaudeKeychainRead::Payload {
                json: Zeroizing::new(replacement.to_owned()),
            }
        },
        || !revoked.get(),
    );

    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401)
        })
    ));
    assert_eq!(fetch_count.get(), 1);
    assert_eq!(reread_count.get(), 1);
    assert!(
        cached_credential()
            .payload(service)
            .is_some_and(|payload| payload.as_str() == original)
    );
    assert_eq!(resolved.access_token.as_str(), "old-token");
    clear_bootstrapped_claude_credential();
}

#[test]
fn retry_429_metadata_survives_consent_revocation_after_response() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    let replacement = r#"{"claudeAiOauth":{"accessToken":"new-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        next_generation(),
    );
    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let revoked = Cell::new(false);
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            let attempt = fetch_count.get();
            fetch_count.set(attempt + 1);
            if attempt == 0 {
                Err(crate::usage::ProviderHttpError::HttpStatus {
                    status: 401,
                    message: "unauthorized".to_owned(),
                    retry_after_seconds: None,
                    response_received_at_epoch: None,
                })
            } else {
                revoked.set(true);
                Err(crate::usage::ProviderHttpError::HttpStatus {
                    status: 429,
                    message: "rate limited".to_owned(),
                    retry_after_seconds: Some(120),
                    response_received_at_epoch: Some(1_000),
                })
            }
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Payload {
                json: Zeroizing::new(replacement.to_owned()),
            }
        },
        || !revoked.get(),
    );

    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::Provider(
            crate::usage::ProviderHttpError::HttpStatus {
                status: 429,
                retry_after_seconds: Some(120),
                response_received_at_epoch: Some(1_000),
                ..
            }
        ))
    ));
    assert_eq!(fetch_count.get(), 2);
    assert_eq!(reread_count.get(), 1);
    assert!(
        cached_credential()
            .payload(service)
            .is_some_and(|payload| payload.as_str() == original)
    );
    clear_bootstrapped_claude_credential();
}

#[test]
fn typed_403_and_unchanged_401_never_trigger_a_second_fetch() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"same-token"}}"#;

    let generation = next_generation();
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        generation,
    );
    let mut resolved = resolved_from_payload(original, service);
    let fetch_count = Cell::new(0);
    let reread_count = Cell::new(0);
    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            fetch_count.set(fetch_count.get() + 1);
            Err(crate::usage::ProviderHttpError::HttpStatus {
                status: 403,
                message: "forbidden".to_owned(),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            })
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Payload {
                json: Zeroizing::new(original.to_owned()),
            }
        },
        || true,
    );
    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::Provider(
            crate::usage::ProviderHttpError::HttpStatus { status: 403, .. }
        ))
    ));
    assert_eq!(fetch_count.get(), 1);
    assert_eq!(reread_count.get(), 0);

    let result = super::super::fetch_claude_with_one_401_reread(
        service,
        &mut resolved,
        |_| {
            fetch_count.set(fetch_count.get() + 1);
            Err(crate::usage::ProviderHttpError::HttpStatus {
                status: 401,
                message: "unauthorized".to_owned(),
                retry_after_seconds: None,
                response_received_at_epoch: None,
            })
        },
        |_| {
            reread_count.set(reread_count.get() + 1);
            ClaudeKeychainRead::Payload {
                json: Zeroizing::new(original.to_owned()),
            }
        },
        || true,
    );
    assert!(matches!(
        result,
        Err(super::super::ClaudeFetchError::Provider(
            crate::usage::ProviderHttpError::HttpStatus { status: 401, .. }
        ))
    ));
    assert_eq!(fetch_count.get(), 2);
    assert_eq!(reread_count.get(), 1);
    clear_bootstrapped_claude_credential();
}
