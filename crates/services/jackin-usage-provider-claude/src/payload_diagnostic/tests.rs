// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::{MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES, diagnose_claude_profile_payload};
use serde_json::Value;
use std::collections::BTreeSet;

fn diagnose(payload: &[u8]) -> Value {
    serde_json::to_value(diagnose_claude_profile_payload(payload)).expect("diagnostic serializes")
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn diagnostic_classifies_json_validity_and_root_kinds() {
    for (payload, expected_kind) in [
        (b"null".as_slice(), "null"),
        (b"{}".as_slice(), "object"),
        (br#""root""#.as_slice(), "string"),
        (b"17".as_slice(), "number"),
        (b"false".as_slice(), "boolean"),
        (b"[]".as_slice(), "array"),
    ] {
        let diagnostic = diagnose(payload);
        assert_eq!(diagnostic["json"], "valid");
        assert_eq!(diagnostic["root"], expected_kind);
    }

    let invalid = diagnose(b"{");
    assert_eq!(invalid["json"], "invalid");
    assert_eq!(invalid["root"], "unavailable");

    let missing = diagnose(br#"{"claudeAiOauth":{}}"#);
    assert_eq!(missing["json"], "valid");
    assert_eq!(missing["root"], "object");
    assert_eq!(missing["access_token"]["camel_case"], "missing");
    assert_eq!(missing["access_token"]["snake_case"], "missing");
}

#[test]
fn diagnostic_reports_optional_metadata_json_kinds_without_values() {
    let payload = br#"{
        "claudeAiOauth": {
            "accessToken": "fixture-token",
            "subscriptionType": 17,
            "rate_limit_tier": []
        },
        "oauth_account": {
            "email_address": false,
            "organizationType": {}
        }
    }"#;
    assert!(crate::parse_claude_keychain_profile(payload).is_none());
    let diagnostic = diagnose(payload);

    assert_eq!(diagnostic["oauth_container"]["camel_case"], "object");
    assert_eq!(diagnostic["oauth_container"]["snake_case"], "missing");
    assert_eq!(diagnostic["access_token"]["camel_case"], "string");
    assert_eq!(diagnostic["access_token"]["camel_case_nonempty"], true);
    assert_eq!(
        diagnostic["subscription_type"]["subscription_type"],
        "number"
    );
    assert_eq!(
        diagnostic["subscription_type"]["rate_limit_tier_snake_case"],
        "array"
    );
    assert_eq!(diagnostic["account_container"]["snake_case"], "object");
    assert_eq!(diagnostic["email_address"]["snake_case"], "boolean");
    assert_eq!(diagnostic["organization_type"]["camel_case"], "object");
}

#[test]
fn diagnostic_accepts_single_snake_case_spellings() {
    let payload = br#"{
        "claude_ai_oauth": {
            "access_token": "fixture-token",
            "rate_limit_tier": "max"
        },
        "oauth_account": {
            "email_address": "fixture-person@example.test",
            "organization_type": "claude_team"
        }
    }"#;
    assert!(crate::parse_claude_keychain_profile(payload).is_some());
    let diagnostic = diagnose(payload);

    assert_eq!(diagnostic["oauth_container"]["camel_case"], "missing");
    assert_eq!(diagnostic["oauth_container"]["snake_case"], "object");
    assert_eq!(diagnostic["oauth_container"]["duplicate_alias"], false);
    assert_eq!(diagnostic["access_token"]["camel_case"], "missing");
    assert_eq!(diagnostic["access_token"]["snake_case"], "string");
    assert_eq!(diagnostic["access_token"]["snake_case_nonempty"], true);
    assert_eq!(
        diagnostic["subscription_type"]["rate_limit_tier_snake_case"],
        "string"
    );
    assert_eq!(diagnostic["account_container"]["snake_case"], "object");
    assert_eq!(diagnostic["email_address"]["snake_case"], "string");
    assert_eq!(diagnostic["organization_type"]["snake_case"], "string");
}

#[test]
fn parser_accepts_distinct_subscription_and_rate_limit_metadata() {
    let mut payload = br#"{"claudeAiOauth":{"accessToken":"fixture-token","subscriptionType":"claude_team","rateLimitTier":"max"}}"#.to_vec();
    payload.resize(524, b' ');

    let parsed = crate::parse_claude_keychain_profile(&payload).expect("valid credential shape");
    let credential = parsed.credential.expect("nonblank access token");
    assert_eq!(credential.subscription_type.as_deref(), Some("Claude Team"));

    let diagnostic = diagnose(&payload);
    assert_eq!(diagnostic["payload_bytes"], 524);
    assert_eq!(
        diagnostic["subscription_type"]["subscription_type"],
        "string"
    );
    assert_eq!(diagnostic["subscription_type"]["rate_limit_tier"], "string");
    assert_eq!(diagnostic["subscription_type"]["duplicate_alias"], false);
}

#[test]
fn parser_falls_back_to_rate_limit_tier_and_allows_missing_or_null_metadata() {
    for (payload, expected) in [
        (
            br#"{"claudeAiOauth":{"accessToken":"fixture-token","subscription_type":"claude_team","rate_limit_tier":"max"}}"#.as_slice(),
            Some("Claude Team"),
        ),
        (
            br#"{"claudeAiOauth":{"accessToken":"fixture-token","rateLimitTier":"claude_max"}}"#.as_slice(),
            Some("Claude Max"),
        ),
        (
            br#"{"claudeAiOauth":{"accessToken":"fixture-token","subscriptionType":null,"rateLimitTier":"claude_pro"}}"#.as_slice(),
            Some("Claude Pro"),
        ),
        (
            br#"{"claudeAiOauth":{"accessToken":"fixture-token"}}"#.as_slice(),
            None,
        ),
        (
            br#"{"claudeAiOauth":{"accessToken":"fixture-token","subscriptionType":null,"rateLimitTier":null}}"#.as_slice(),
            None,
        ),
    ] {
        let parsed = crate::parse_claude_keychain_profile(payload).expect("valid credential shape");
        assert_eq!(
            parsed
                .credential
                .expect("nonblank access token")
                .subscription_type
                .as_deref(),
            expected
        );
    }

    let wrong_type = br#"{"claudeAiOauth":{"accessToken":"fixture-token","subscriptionType":17,"rateLimitTier":"max"}}"#;
    assert!(crate::parse_claude_keychain_profile(wrong_type).is_none());
}

#[test]
fn parser_rejects_duplicate_spellings_of_the_same_metadata_field() {
    for payload in [
        br#"{"claudeAiOauth":{"accessToken":"fixture-token","subscriptionType":"team","subscription_type":"max"}}"#.as_slice(),
        br#"{"claudeAiOauth":{"accessToken":"fixture-token","rateLimitTier":"team","rate_limit_tier":"max"}}"#.as_slice(),
    ] {
        assert!(crate::parse_claude_keychain_profile(payload).is_none());
        assert_eq!(
            diagnose(payload)["subscription_type"]["duplicate_alias"],
            true
        );
    }
}

#[test]
fn diagnostic_reports_camel_snake_aliases_and_collisions() {
    let payload = br#"{
            "claudeAiOauth": {
                "accessToken": "fixture-token",
                "access_token": "other-fixture-token",
                "subscriptionType": "team",
                "rateLimitTier": "max"
            },
            "claude_ai_oauth": {"accessToken": "ignored-fixture-token"},
            "oauthAccount": {
                "emailAddress": "first@example.test",
                "email_address": "second@example.test",
                "organizationType": "team",
                "organization_type": "enterprise"
            },
            "oauth_account": {}
        }"#;
    assert!(crate::parse_claude_keychain_profile(payload).is_none());
    let diagnostic = diagnose(payload);

    assert_eq!(diagnostic["oauth_container"]["camel_case"], "object");
    assert_eq!(diagnostic["oauth_container"]["snake_case"], "object");
    assert_eq!(diagnostic["oauth_container"]["duplicate_alias"], true);
    assert_eq!(diagnostic["access_token"]["camel_case"], "string");
    assert_eq!(diagnostic["access_token"]["snake_case"], "string");
    assert_eq!(diagnostic["access_token"]["duplicate_alias"], true);
    assert_eq!(diagnostic["subscription_type"]["duplicate_alias"], false);
    assert_eq!(diagnostic["account_container"]["duplicate_alias"], true);
    assert_eq!(diagnostic["email_address"]["duplicate_alias"], true);
    assert_eq!(diagnostic["organization_type"]["duplicate_alias"], true);
}

#[test]
fn diagnostic_obeys_payload_size_boundary_before_json_parsing() {
    let mut at_limit = br#"{"claudeAiOauth":{"accessToken":"fixture-token"}}"#.to_vec();
    at_limit.resize(MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES, b' ');
    let exact = diagnose(&at_limit);
    assert_eq!(exact["payload_bytes"], MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES);
    assert_eq!(exact["limit_bytes"], MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES);
    assert_eq!(exact["json"], "valid");
    assert_eq!(exact["root"], "object");

    let oversized = vec![b'{'; MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES + 1];
    let over = diagnose(&oversized);
    assert_eq!(over["payload_bytes"], MAX_CLAUDE_KEYCHAIN_PAYLOAD_BYTES + 1);
    assert_eq!(over["json"], "skipped_oversize");
    assert_eq!(over["root"], "unavailable");
    assert_eq!(over["access_token"]["camel_case"], "unavailable");
}

#[test]
fn diagnostic_schema_is_fixed_and_never_serializes_payload_values() {
    let payload = br#"{
        "claudeAiOauth": {"accessToken": "fixture-secret-token"},
        "oauthAccount": {"emailAddress": "fixture-person@example.test"},
        "unknownPrivateField": "fixture-unknown-secret"
    }"#;
    let diagnostic = diagnose(payload);
    let encoded = diagnostic.to_string();

    assert!(!encoded.contains("fixture-secret-token"));
    assert!(!encoded.contains("fixture-person@example.test"));
    assert!(!encoded.contains("unknownPrivateField"));
    assert!(!encoded.contains("fixture-unknown-secret"));
    assert_eq!(
        keys(&diagnostic),
        BTreeSet::from([
            "payload_bytes",
            "limit_bytes",
            "json",
            "root",
            "oauth_container",
            "access_token",
            "subscription_type",
            "account_container",
            "email_address",
            "organization_type",
        ])
    );
    for field in [
        "oauth_container",
        "account_container",
        "email_address",
        "organization_type",
    ] {
        assert_eq!(
            keys(&diagnostic[field]),
            BTreeSet::from(["camel_case", "snake_case", "duplicate_alias"])
        );
    }
    assert_eq!(
        keys(&diagnostic["access_token"]),
        BTreeSet::from([
            "camel_case",
            "snake_case",
            "camel_case_nonempty",
            "snake_case_nonempty",
            "duplicate_alias",
        ])
    );
    assert_eq!(
        keys(&diagnostic["subscription_type"]),
        BTreeSet::from([
            "subscription_type",
            "subscription_type_snake_case",
            "rate_limit_tier",
            "rate_limit_tier_snake_case",
            "duplicate_alias",
        ])
    );
}

#[test]
fn escaped_access_token_is_accepted_by_parser_but_diagnostic_leaves_nonempty_unknown() {
    let payload = br#"{"claudeAiOauth":{"accessToken":"fixture\u002dtoken"}}"#;
    let parsed = crate::parse_claude_keychain_profile(payload).expect("valid JSON profile");
    assert!(parsed.credential.is_some());

    let diagnostic = diagnose(payload);
    assert_eq!(diagnostic["access_token"]["camel_case"], "string");
    assert_eq!(
        diagnostic["access_token"]["camel_case_nonempty"],
        Value::Null
    );
}

#[test]
fn missing_and_blank_tokens_have_distinct_safe_facts() {
    let missing = diagnose(br#"{"claudeAiOauth":{}}"#);
    assert_eq!(missing["access_token"]["camel_case"], "missing");
    assert_eq!(missing["access_token"]["camel_case_nonempty"], Value::Null);

    let blank = diagnose(br#"{"claudeAiOauth":{"accessToken":"  "}}"#);
    assert_eq!(blank["access_token"]["camel_case"], "string");
    assert_eq!(blank["access_token"]["camel_case_nonempty"], false);

    let null = diagnose(br#"{"claudeAiOauth":{"accessToken":null}}"#);
    assert_eq!(null["access_token"]["camel_case"], "null");
    assert_eq!(null["access_token"]["camel_case_nonempty"], Value::Null);
}
