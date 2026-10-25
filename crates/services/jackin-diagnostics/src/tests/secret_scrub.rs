// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::secret_scrub::scrub_secrets;
use std::borrow::Cow;

#[test]
fn clean_text_passes_through_unchanged() {
    let input = "build finished in 12s with no warnings";
    assert!(matches!(scrub_secrets(input), Cow::Borrowed(_)));
    assert_eq!(scrub_secrets(input).into_owned(), input);
}

#[test]
fn github_pat_is_redacted_but_surroundings_survive() {
    let output = scrub_secrets("sync token ghp_abcdefghijklmnopqr done").into_owned();
    assert!(!output.contains("ghp_abcdefghijklmnopqr"));
    assert!(output.contains("redacted"));
    assert!(output.contains("done"));
}

#[test]
fn pem_private_key_block_is_redacted_but_surroundings_survive() {
    let input =
        "header\n-----BEGIN PRIVATE KEY-----\nMIIBvTCCASs=\n-----END PRIVATE KEY-----\ntrailer";
    let output = scrub_secrets(input).into_owned();
    assert!(!output.contains("BEGIN PRIVATE KEY"));
    assert!(output.contains("redacted"));
    assert!(output.contains("trailer"));
}
