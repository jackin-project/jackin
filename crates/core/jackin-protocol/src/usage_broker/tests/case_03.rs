// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::control::UsageProviderTab;

fn capability(surface_id: &str, account_id: &str) -> UsageAccountCapability {
    UsageAccountCapability {
        account_id: account_id.to_owned(),
        surface_id: surface_id.to_owned(),
    }
}

fn tab(capability: UsageAccountCapability, account_label: &str) -> UsageProviderTab {
    UsageProviderTab {
        id: "unused-display-id".to_owned(),
        account_capability: Some(capability),
        label: "OpenAI".to_owned(),
        status_label: "fresh".to_owned(),
        account_label: account_label.to_owned(),
        plan_label: None,
        source_label: None,
        active: false,
    }
}

#[test]
fn opaque_identity_key_has_a_stable_domain_separated_golden_value() {
    let selected = capability("surface-a", "acct-a");
    assert_eq!(
        selected.opaque_identity_key(),
        "capability-v1:sha256:b1de09378b75758fa7a3fd263db56588d4af3f35adccb032cac44d5a68997654"
    );
    assert_eq!(
        selected.opaque_identity_key(),
        selected.opaque_identity_key()
    );

    let legacy_label_key = jackin_core::account_key_hash("provider", "Shared label");
    assert!(legacy_label_key.starts_with("sha256:"));
    assert_ne!(selected.opaque_identity_key(), legacy_label_key);
    assert!(
        selected
            .opaque_identity_key()
            .starts_with("capability-v1:sha256:")
    );
}

#[test]
fn opaque_identity_key_frames_the_exact_ordered_tuple_as_utf8_bytes() {
    assert_ne!(
        capability("surface-a", "same-account").opaque_identity_key(),
        capability("surface-b", "same-account").opaque_identity_key()
    );
    assert_ne!(
        capability("surface-a", "first-account").opaque_identity_key(),
        capability("surface-a", "second-account").opaque_identity_key()
    );
    assert_ne!(
        capability("surface-a", "acct-a").opaque_identity_key(),
        capability("acct-a", "surface-a").opaque_identity_key()
    );

    assert_ne!(
        capability("a", "bc").opaque_identity_key(),
        capability("ab", "c").opaque_identity_key()
    );
    assert_ne!(
        capability("1:a", "2:b").opaque_identity_key(),
        capability("1", "a2:b").opaque_identity_key()
    );

    assert_eq!(
        capability("é", "x").opaque_identity_key(),
        "capability-v1:sha256:5b7745689b2bcf19cadbc47b3e07fd50d89630f43d27deec4f73befbd0d3f31d"
    );
    assert_ne!(
        capability("é", "x").opaque_identity_key(),
        capability("e\u{301}", "x").opaque_identity_key()
    );
    assert_ne!(
        capability("Surface-A", "acct-a").opaque_identity_key(),
        capability("surface-a", "acct-a").opaque_identity_key()
    );
}

#[test]
fn opaque_identity_key_is_independent_of_display_label() {
    let selected = capability("surface-a", "acct-a");
    let first = tab(selected.clone(), "Shared label");
    let renamed = tab(selected, "Renamed label");

    assert_eq!(
        first
            .account_capability
            .as_ref()
            .expect("bound tab has selector metadata")
            .opaque_identity_key(),
        renamed
            .account_capability
            .as_ref()
            .expect("renamed tab retains selector metadata")
            .opaque_identity_key()
    );
    assert_ne!(first.account_label, renamed.account_label);
}
