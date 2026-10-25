// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::{claude_email_from_value, diagnose_claude_profile_payload};

#[test]
fn email_prefers_camel_case_address() {
    let value = serde_json::json!({
        "oauthAccount": {
            "emailAddress": "camel@example.com",
            "email_address": "snake@example.com",
        }
    });

    assert_eq!(
        claude_email_from_value(&value).as_deref(),
        Some("camel@example.com")
    );
}

#[test]
fn email_falls_back_to_snake_case_and_trims() {
    let value = serde_json::json!({
        "oauthAccount": { "email_address": "  snake@example.com  " }
    });

    assert_eq!(
        claude_email_from_value(&value).as_deref(),
        Some("snake@example.com")
    );
}

#[test]
fn email_is_absent_without_usable_account() {
    assert_eq!(
        claude_email_from_value(&serde_json::json!({})).as_deref(),
        None
    );
    assert_eq!(
        claude_email_from_value(&serde_json::json!({
            "oauthAccount": { "emailAddress": "   " }
        }))
        .as_deref(),
        None
    );
}

#[test]
fn oversize_payload_skips_json_inspection() {
    let bytes = vec![b'{'; 65 * 1024];

    let diagnostic = diagnose_claude_profile_payload(&bytes);
    let value =
        serde_json::to_value(&diagnostic).expect("diagnostic must serialize for inspection");

    assert_eq!(value["json"].as_str(), Some("skipped_oversize"));
    assert_eq!(value["payload_bytes"].as_u64(), Some(65 * 1024));
    assert_eq!(value["limit_bytes"].as_u64(), Some(64 * 1024));
}

#[test]
fn small_malformed_payload_reports_invalid_json() {
    let diagnostic = diagnose_claude_profile_payload(b"{not json");

    let value =
        serde_json::to_value(&diagnostic).expect("diagnostic must serialize for inspection");

    assert_eq!(value["json"].as_str(), Some("invalid"));
}

#[test]
fn small_empty_object_payload_reports_valid_json() {
    let diagnostic = diagnose_claude_profile_payload(b"{}");

    let value =
        serde_json::to_value(&diagnostic).expect("diagnostic must serialize for inspection");

    assert_eq!(value["json"].as_str(), Some("valid"));
}
