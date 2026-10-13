// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn debug_redacts_secret_value() {
    let candidate = StoreCandidate::new(
        StoreKind::Opencode,
        "anthropic".to_owned(),
        None,
        PathBuf::from("/tmp/auth.json"),
        CredentialKind::ApiKey,
        "key".to_owned(),
        "fixture-credential-001".to_owned(),
    );
    let rendered = format!("{candidate:?}");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("fixture-credential-001"));
    assert!(rendered.contains("anthropic"));
    // The secret is stored (equality distinguishes it) but never shown.
    let other = StoreCandidate::new(
        StoreKind::Opencode,
        "anthropic".to_owned(),
        None,
        PathBuf::from("/tmp/auth.json"),
        CredentialKind::ApiKey,
        "key".to_owned(),
        "fixture-credential-002".to_owned(),
    );
    assert_ne!(candidate, other);
    assert_eq!(candidate, candidate.clone());
}

#[test]
fn select_entry_secret_covers_shared_shapes() {
    let api: serde_json::Value = serde_json::from_str(r#"{"type":"api","key":"k"}"#).unwrap();
    assert_eq!(
        select_entry_secret(&api),
        Some((CredentialKind::ApiKey, "key", "k"))
    );
    let oauth: serde_json::Value =
        serde_json::from_str(r#"{"type":"oauth","access":"a","refresh":"r"}"#).unwrap();
    assert_eq!(
        select_entry_secret(&oauth),
        Some((CredentialKind::OAuth, "access", "a"))
    );
    let refresh_only: serde_json::Value =
        serde_json::from_str(r#"{"type":"oauth","refresh":"r"}"#).unwrap();
    assert_eq!(
        select_entry_secret(&refresh_only),
        Some((CredentialKind::OAuth, "refresh", "r"))
    );
    for raw in [
        r#"{"type":"api","key":"  "}"#,
        r#"{"type":"oauth"}"#,
        r#"{"type":"unknown","key":"k"}"#,
        r#"{"key":"k"}"#,
        r"[]",
    ] {
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(select_entry_secret(&value), None, "raw: {raw}");
    }
}
