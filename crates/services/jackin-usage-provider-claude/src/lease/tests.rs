// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

fn serialized_bootstrap() -> std::sync::MutexGuard<'static, ()> {
    serialized_credential_cache_test()
}

fn valid_payload(token: &str) -> Zeroizing<String> {
    Zeroizing::new(format!(
        r#"{{"claudeAiOauth":{{"accessToken":"{token}"}}}}"#
    ))
}

#[test]
fn source_partition_hash_preserves_exact_service_bytes() {
    let exact = "Claude Code-credentials-91a2bc34";
    let expected = jackin_core::account_key_hash("claude-keychain-service-v1", exact);
    assert_eq!(
        claude_source_capability_id_for_service(exact),
        expected.strip_prefix("sha256:").unwrap_or(&expected)
    );
    assert_ne!(
        claude_source_capability_id_for_service(exact),
        claude_source_capability_id_for_service(" Claude Code-credentials-91a2bc34")
    );
    assert!(valid_claude_keychain_service(exact));
    assert!(!valid_claude_keychain_service("  "));
    assert!(!valid_claude_keychain_service("Claude\0Code"));
    assert!(!valid_claude_keychain_service(
        &"x".repeat(MAX_CLAUDE_KEYCHAIN_SERVICE_BYTES + 1)
    ));
}

#[test]
fn bootstrap_fails_before_keychain_access_without_tty_and_validates_selected_source() {
    let read_count = Mutex::new(0);
    let result = bootstrap_claude_credential_with("selected", false, || {
        *read_count.lock().unwrap() += 1;
        ClaudeKeychainRead::Payload {
            json: valid_payload("fixture-token"),
        }
    })
    .expect("headless bootstrap is a typed outcome");
    assert!(matches!(
        result,
        ClaudeCredentialBootstrapOutcome::InteractionRequired
    ));
    assert_eq!(*read_count.lock().unwrap(), 0);

    assert!(matches!(
        bootstrap_claude_credential_with(" ", true, || panic!("invalid service must stop")),
        Err(ClaudeKeychainPolicyError::InvalidService)
    ));
}

#[test]
fn bootstrap_retains_one_bounded_zeroizing_exact_source_until_last_lease_drops() {
    let _serial = serialized_bootstrap();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let outcome = bootstrap_claude_credential_with(service, true, || ClaudeKeychainRead::Payload {
        json: valid_payload("selected-token"),
    })
    .expect("valid bootstrap");
    let ClaudeCredentialBootstrapOutcome::Acquired(lease) = outcome else {
        panic!("valid fixture credential should acquire a lease")
    };
    assert_eq!(
        lease.source_capability_id(),
        claude_source_capability_id_for_service(service)
    );
    assert!(!format!("{lease:?}").contains("selected-token"));
    assert_eq!(
        cached_payload_for_lease(&lease)
            .as_deref()
            .map(String::as_str),
        Some(r#"{"claudeAiOauth":{"accessToken":"selected-token"}}"#)
    );
    assert!(cached_claude_keychain_payload("other-service").is_none());
    assert!(matches!(
        bootstrap_claude_credential_with("other-service", true, || panic!(
            "second source must stop"
        )),
        Err(ClaudeKeychainPolicyError::ScopeConflict)
    ));

    let cloned = lease.clone();
    drop(lease);
    assert!(cached_claude_keychain_payload(service).is_some());
    drop(cloned);
    assert!(cached_claude_keychain_payload(service).is_none());
}

#[test]
fn bootstrap_rejects_oversized_and_noncredential_payloads() {
    let _serial = serialized_bootstrap();
    clear_bootstrapped_claude_credential();
    let oversized = Zeroizing::new(format!(
        r#"{{"claudeAiOauth":{{"accessToken":"token"}}}}{}"#,
        " ".repeat(MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES)
    ));
    for payload in [oversized, Zeroizing::new("not-json".to_owned())] {
        let outcome = bootstrap_claude_credential_with("selected", true, || {
            ClaudeKeychainRead::Payload { json: payload }
        })
        .expect("malformed payload is a typed outcome");
        assert!(matches!(
            outcome,
            ClaudeCredentialBootstrapOutcome::Malformed
        ));
        clear_bootstrapped_claude_credential();
    }
}

#[test]
fn one_401_reread_replaces_only_the_selected_generation_once() {
    let _serial = serialized_bootstrap();
    clear_bootstrapped_claude_credential();
    let service = "Claude Code-credentials-selected";
    let outcome = bootstrap_claude_credential_with(service, true, || ClaudeKeychainRead::Payload {
        json: valid_payload("old-token"),
    })
    .expect("valid bootstrap");
    let ClaudeCredentialBootstrapOutcome::Acquired(lease) = outcome else {
        panic!("valid fixture credential should acquire a lease")
    };

    assert!(begin_unauthorized_reread(&lease));
    assert!(!begin_unauthorized_reread(&lease));
    assert!(replace_if_exact(&lease, valid_payload("new-token")));
    assert_eq!(
        cached_payload_for_lease(&lease)
            .as_deref()
            .map(String::as_str),
        Some(r#"{"claudeAiOauth":{"accessToken":"new-token"}}"#)
    );
    drop(lease);
    assert!(bootstrapped_claude_service().is_none());
}

#[test]
fn stale_generation_cannot_read_or_replace_newer_cached_credential() {
    let service = "Claude Code-credentials-selected";
    let cache = ClaudeCredentialCache::default();
    let stale_generation = 41;
    let current_generation = 42;
    cache.store(
        service.to_owned(),
        valid_payload("generation-41"),
        stale_generation,
    );
    assert!(cache.replace_if_exact(service, stale_generation, valid_payload("replacement-41")));
    assert_eq!(
        cache
            .payload(service, Some(stale_generation))
            .as_deref()
            .map(String::as_str),
        Some(r#"{"claudeAiOauth":{"accessToken":"replacement-41"}}"#)
    );

    cache.store(
        service.to_owned(),
        valid_payload("generation-42"),
        current_generation,
    );
    assert!(cache.payload(service, Some(stale_generation)).is_none());
    assert!(!cache.replace_if_exact(
        service,
        stale_generation,
        valid_payload("stale-replacement")
    ));
    assert_eq!(
        cache
            .payload(service, Some(current_generation))
            .as_deref()
            .map(String::as_str),
        Some(r#"{"claudeAiOauth":{"accessToken":"generation-42"}}"#)
    );
}
