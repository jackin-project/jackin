// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn local_source_identity_label_does_not_claim_provider_identity() {
    assert_eq!(
        identity_kind_label(UsageIdentityKindV1::LocalSourceHandle),
        "local source handle"
    );
}

#[test]
fn unverified_identity_label_does_not_claim_provider_or_local_identity() {
    assert_eq!(
        identity_kind_label(UsageIdentityKindV1::UnverifiedHandle),
        "unverified handle"
    );
}
