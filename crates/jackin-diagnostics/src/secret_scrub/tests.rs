//! Tests for diagnostic secret-shape scrubbing.

use super::*;

use std::borrow::Cow;

#[test]
fn scrub_secrets_masks_known_token_shapes() {
    let text = concat!(
        "github=ghp_1234567890abcdef\n",
        "oauth=gho_1234567890abcdef\n",
        "server=ghs_1234567890abcdef\n",
        "llm=sk-ant-api03-1234567890abcdef\n",
        "aws=AKIA1234567890ABCDEF\n",
        "op=op://Private Vault/item/field\n",
    );

    let redacted = scrub_secrets(text);
    assert!(!redacted.contains("ghp_1234567890abcdef"));
    assert!(!redacted.contains("gho_1234567890abcdef"));
    assert!(!redacted.contains("ghs_1234567890abcdef"));
    assert!(!redacted.contains("sk-ant-api03-1234567890abcdef"));
    assert!(!redacted.contains("AKIA1234567890ABCDEF"));
    assert!(!redacted.contains("op://Private"));
    assert!(redacted.contains(VALUE_MARKER));
}

#[test]
fn scrub_secrets_masks_pem_key_blocks() {
    let text = "before\n-----BEGIN PRIVATE KEY-----\nMIIsecret\n-----END PRIVATE KEY-----\nafter";

    let redacted = scrub_secrets(text);
    assert!(!redacted.contains("MIIsecret"));
    assert!(redacted.contains(KEY_MARKER));
    assert!(redacted.contains("before"));
    assert!(redacted.contains("after"));
}

#[test]
fn scrub_secrets_masks_secret_assignments() {
    let redacted = scrub_secrets("GITHUB_TOKEN=ghp_1234567890abcdef normal=value");

    assert!(!redacted.contains("ghp_1234567890abcdef"));
    assert!(redacted.contains("GITHUB_TOKEN=<secret redacted>"));
    assert!(redacted.contains("normal=value"));
}

#[test]
fn scrub_secrets_leaves_normal_text_untouched() {
    let text = "step 7/12: copying assets for workspace";

    assert!(matches!(scrub_secrets(text), Cow::Borrowed(_)));
}
