// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

fn test_lease_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct DropTrackedGuard(Arc<AtomicBool>);

impl Drop for DropTrackedGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
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
    assert_eq!(selected.as_str(), "selected-token");
    assert!(cache.payload("Claude Code-credentials-b").is_none());
    assert!(cache.begin_unauthorized_reread("Claude Code-credentials-a", 5));
    assert!(cache.replace_if_exact(
        "Claude Code-credentials-a",
        5,
        Zeroizing::new("rotated-token".to_owned())
    ));
    assert!(!cache.begin_unauthorized_reread("Claude Code-credentials-a", 5));
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
    let mut valid = String::from(
        r#"{"claudeAiOauth":{"accessToken":"selected-token","subscriptionType":"team","rateLimitTier":"max"}}"#,
    );
    valid.push_str(&" ".repeat(524 - valid.len()));
    assert_eq!(valid.len(), 524);
    valid_claude_keychain_payload(&valid).unwrap();
    assert!(valid_claude_keychain_payload(r#"{"claudeAiOauth":{}}"#).is_err());
    assert!(valid_claude_keychain_payload("not-json").is_err());
    let oversized_payload = format!("{valid}{}", " ".repeat(MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES));
    let oversized_diagnostic = valid_claude_keychain_payload(&oversized_payload)
        .expect_err("oversized payload is rejected");
    assert_eq!(oversized_diagnostic.payload_bytes, oversized_payload.len());
    assert_eq!(
        oversized_diagnostic.json,
        super::super::ClaudePayloadJsonState::SkippedOversize
    );
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

    result.expect("a changed exact-source credential succeeds after one retry");
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
fn stop_before_retry_admission_skips_fetch_after_changed_credential_reread() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    let replacement = r#"{"claudeAiOauth":{"accessToken":"new-token"}}"#;
    let generation = next_generation();
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        generation,
    );
    let lease = ClaudeCredentialLease {
        service: service.to_owned(),
        generation,
    };
    let liveness = Arc::new(crate::usage::ClaudeCollectorLiveness::new(()));
    liveness.bind_generation(generation);
    let fetch_count = Arc::new(AtomicUsize::new(0));
    let reread_count = Arc::new(AtomicUsize::new(0));
    let admission_count = Arc::new(AtomicUsize::new(0));
    let (retry_admission_tx, retry_admission_rx) = mpsc::channel();
    let (retry_admission_release_tx, retry_admission_release_rx) = mpsc::channel();
    let mut resolved = resolved_from_payload(original, service);

    let task_liveness = Arc::clone(&liveness);
    let task_current_liveness = Arc::clone(&liveness);
    let task_fetch_count = Arc::clone(&fetch_count);
    let task_reread_count = Arc::clone(&reread_count);
    let task_admission_count = Arc::clone(&admission_count);
    let task = std::thread::spawn(move || {
        super::super::fetch_claude_with_one_401_reread_with_admission(
            service,
            &mut resolved,
            move |_| {
                let attempt = task_fetch_count.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
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
            move |_| {
                task_reread_count.fetch_add(1, Ordering::SeqCst);
                ClaudeKeychainRead::Payload {
                    json: Zeroizing::new(replacement.to_owned()),
                }
            },
            move || {
                let attempt = task_admission_count.fetch_add(1, Ordering::SeqCst);
                if attempt == 2 {
                    retry_admission_tx
                        .send(())
                        .expect("signal before retry admission");
                    retry_admission_release_rx
                        .recv()
                        .expect("release retry admission after stop");
                }
                task_liveness.admit_if(|generation| {
                    claude_credential_generation_is_current(service, generation)
                })
            },
            move || {
                task_current_liveness.is_current_if(|generation| {
                    claude_credential_generation_is_current(service, generation)
                })
            },
        )
    });

    retry_admission_rx
        .recv()
        .expect("changed credential reaches the retry admission gate");
    liveness.deactivate();
    drop(lease);
    assert!(cached_credential().payload(service).is_none());
    let newer_generation = next_generation();
    let newer_payload = r#"{"claudeAiOauth":{"accessToken":"newer-session-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(newer_payload.to_owned()),
        newer_generation,
    );
    drop(liveness);
    retry_admission_release_tx
        .send(())
        .expect("retry admission observes deactivation");

    assert!(matches!(
        task.join().expect("collector exits after denied admission"),
        Err(super::super::ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401)
        })
    ));
    assert_eq!(fetch_count.load(Ordering::SeqCst), 1);
    assert_eq!(reread_count.load(Ordering::SeqCst), 1);
    assert_eq!(admission_count.load(Ordering::SeqCst), 3);
    assert!(
        cached_credential()
            .payload(service)
            .is_some_and(|payload| payload.as_str() == newer_payload)
    );
    clear_bootstrapped_claude_credential();
}

#[test]
fn revoked_generation_cannot_replace_cache_or_clear_newer_generation() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    let old_generation = next_generation();
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        old_generation,
    );
    let (write_ready_tx, write_ready_rx) = mpsc::channel();
    let (write_release_tx, write_release_rx) = mpsc::channel();
    let delayed_write = std::thread::spawn(move || {
        write_ready_tx.send(()).expect("signal before cache write");
        write_release_rx
            .recv()
            .expect("release delayed cache write after revocation");
        replace_bootstrapped_claude_payload(
            service,
            old_generation,
            Zeroizing::new(r#"{"claudeAiOauth":{"accessToken":"stale-token"}}"#.to_owned()),
        )
    });

    write_ready_rx
        .recv()
        .expect("cache write is paused before its generation check");
    revoke_bootstrapped_claude_generation(old_generation);
    assert!(cached_credential().payload(service).is_none());
    let newer_generation = next_generation();
    let newer_payload = r#"{"claudeAiOauth":{"accessToken":"newer-session-token"}}"#;
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(newer_payload.to_owned()),
        newer_generation,
    );
    assert!(!begin_bootstrapped_claude_401_reread(
        service,
        old_generation
    ));
    write_release_tx
        .send(())
        .expect("stale cache write checks its old generation");

    assert!(
        !delayed_write
            .join()
            .expect("cache replacement completes without writing stale data")
    );
    assert!(
        cached_credential()
            .payload(service)
            .is_some_and(|payload| payload.as_str() == newer_payload)
    );
    clear_bootstrapped_claude_credential();
}

#[test]
fn service_stop_and_lease_release_during_blocked_401_reread_skip_retry_and_keep_no_ui_guard() {
    let _serial = test_lease_lock();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let original = r#"{"claudeAiOauth":{"accessToken":"old-token"}}"#;
    let replacement = r#"{"claudeAiOauth":{"accessToken":"new-token"}}"#;
    let generation = next_generation();
    cached_credential().store(
        service.to_owned(),
        Zeroizing::new(original.to_owned()),
        generation,
    );
    let lease = ClaudeCredentialLease {
        service: service.to_owned(),
        generation,
    };
    let mut resolved = resolved_from_payload(original, service);
    let source_alive = Arc::new(AtomicBool::new(true));
    let guard_dropped = Arc::new(AtomicBool::new(false));
    let liveness = Arc::new(crate::usage::ClaudeCollectorLiveness::new(
        DropTrackedGuard(Arc::clone(&guard_dropped)),
    ));
    liveness.bind_generation(generation);
    let fetch_count = Arc::new(AtomicUsize::new(0));
    let reread_count = Arc::new(AtomicUsize::new(0));
    let (reread_started_tx, reread_started_rx) = mpsc::channel();
    let (reread_release_tx, reread_release_rx) = mpsc::channel();

    let task_liveness = Arc::clone(&liveness);
    let task_admission_liveness = Arc::clone(&liveness);
    let task_source_alive = Arc::clone(&source_alive);
    let task_admission_source_alive = Arc::clone(&source_alive);
    let task_fetch_count = Arc::clone(&fetch_count);
    let task_reread_count = Arc::clone(&reread_count);
    let task = std::thread::spawn(move || {
        super::super::fetch_claude_with_one_401_reread_with_admission(
            service,
            &mut resolved,
            move |_| {
                let attempt = task_fetch_count.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
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
            move |requested_service| {
                assert_eq!(requested_service, service);
                task_reread_count.fetch_add(1, Ordering::SeqCst);
                reread_started_tx
                    .send(())
                    .expect("signal that the bounded reread is blocked");
                reread_release_rx
                    .recv()
                    .expect("release the blocked test reread");
                ClaudeKeychainRead::Payload {
                    json: Zeroizing::new(replacement.to_owned()),
                }
            },
            move || {
                task_admission_liveness.admit_if(|generation| {
                    task_admission_source_alive.load(Ordering::Acquire)
                        && claude_credential_generation_is_current(service, generation)
                })
            },
            move || {
                task_liveness.is_current_if(|generation| {
                    task_source_alive.load(Ordering::Acquire)
                        && claude_credential_generation_is_current(service, generation)
                })
            },
        )
    });

    reread_started_rx
        .recv()
        .expect("first typed 401 starts one exact-service reread");
    assert!(!guard_dropped.load(Ordering::Acquire));

    // Model the synchronous ServiceStop fence, source invalidation, and lease
    // release while the noninteractive Keychain operation is still blocked.
    source_alive.store(false, Ordering::Release);
    liveness.deactivate();
    drop(lease);
    drop(liveness);
    assert!(!guard_dropped.load(Ordering::Acquire));
    reread_release_tx
        .send(())
        .expect("finish the blocked reread without waiting in service teardown");

    assert!(matches!(
        task.join()
            .expect("collector task returns after reread release"),
        Err(super::super::ClaudeFetchError::ConsentRevoked {
            provider_http_status: Some(401)
        })
    ));
    assert_eq!(fetch_count.load(Ordering::SeqCst), 1);
    assert_eq!(reread_count.load(Ordering::SeqCst), 1);
    assert!(guard_dropped.load(Ordering::Acquire));
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
