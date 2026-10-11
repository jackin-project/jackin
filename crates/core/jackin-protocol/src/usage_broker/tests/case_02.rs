// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn account_rejects_duplicate_group_ids_and_bad_group_rank() {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../../../services/jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .unwrap();
    projection.providers[0].accounts[0]
        .metric_groups
        .push(window_group("g", 0));
    projection.providers[0].accounts[0]
        .metric_groups
        .push(window_group("g", 1));
    projection.validate().unwrap_err();
    projection.providers[0].accounts[0].metric_groups[1].group_id = "h".into();
    projection.providers[0].accounts[0].metric_groups[1].rank = 7;
    projection.validate().unwrap_err();
    projection.providers[0].accounts[0].metric_groups[1].rank = 1;
    projection.validate().unwrap();
}

#[test]
fn account_without_metric_groups_stays_wire_compatible() {
    let fixture = include_str!(
        "../../../../../services/jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    );
    let projection: UsageProjectionV1 = serde_json::from_str(fixture).unwrap();
    assert!(projection.providers[0].accounts[0].metric_groups.is_empty());
    assert_eq!(
        projection.providers[0].accounts[0].credential_expires_at_epoch,
        None
    );
    let encoded = serde_json::to_value(&projection).unwrap();
    assert_eq!(
        encoded,
        serde_json::from_str::<serde_json::Value>(fixture).unwrap()
    );
}

#[test]
fn quota_states_serialize_distinctly() {
    let states = [
        (UsageQuotaStateV1::Available, "available"),
        (UsageQuotaStateV1::NotStarted, "not_started"),
        (UsageQuotaStateV1::Warning, "warning"),
        (UsageQuotaStateV1::Exhausted, "exhausted"),
        (UsageQuotaStateV1::Unsupported, "unsupported"),
        (UsageQuotaStateV1::Unavailable, "unavailable"),
        (UsageQuotaStateV1::NoPermission, "no_permission"),
        (UsageQuotaStateV1::Unknown, "unknown"),
        (UsageQuotaStateV1::NotApplicable, "not_applicable"),
        (UsageQuotaStateV1::Error, "error"),
    ];
    let mut seen = BTreeSet::new();
    for (state, wire) in states {
        let encoded = serde_json::to_value(state).unwrap();
        assert_eq!(encoded, serde_json::Value::String(wire.into()));
        assert!(seen.insert(wire));
        assert_eq!(
            serde_json::from_value::<UsageQuotaStateV1>(encoded).unwrap(),
            state
        );
    }
}

#[test]
fn reset_credential_expiry_and_renewal_are_independent_fields() {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../../../services/jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .unwrap();
    let account = &mut projection.providers[0].accounts[0];
    account.windows[0].reset_at_epoch = Some(1_800_100_000);
    account.credential_expires_at_epoch = Some(1_800_200_000);
    let mut plan = window_group("g-plan", 0);
    plan.kind = UsageMetricGroupKindV1::Plan;
    plan.value = UsageMetricValueV1::Plan {
        plan_label: Some("Pro".into()),
        tier: None,
    };
    plan.quota_state = UsageQuotaStateV1::NotApplicable;
    plan.reset_at_epoch = None;
    plan.renews_at_epoch = Some(1_800_300_000);
    account.metric_groups.push(plan);
    projection.validate().unwrap();
    let round_tripped: UsageProjectionV1 =
        serde_json::from_slice(&serde_json::to_vec(&projection).unwrap()).unwrap();
    let account = &round_tripped.providers[0].accounts[0];
    assert_eq!(account.windows[0].reset_at_epoch, Some(1_800_100_000));
    assert_eq!(account.credential_expires_at_epoch, Some(1_800_200_000));
    assert_eq!(
        account.metric_groups[0].renews_at_epoch,
        Some(1_800_300_000)
    );
    assert_eq!(account.metric_groups[0].reset_at_epoch, None);
}

#[test]
fn quota_scope_dedup_key_is_stable_and_axis_sensitive() {
    let base = UsageQuotaScopeKey::new("kimi-code", "org-1", "subscription");
    assert_eq!(base.dedup_key(), base.dedup_key());
    assert!(base.shares_allowance(&UsageQuotaScopeKey::new(
        "kimi-code",
        "org-1",
        "subscription"
    )));
    for other in [
        UsageQuotaScopeKey::new("moonshot-payg", "org-1", "subscription"),
        UsageQuotaScopeKey::new("kimi-code", "org-2", "subscription"),
        UsageQuotaScopeKey::new("kimi-code", "org-1", "key"),
        UsageQuotaScopeKey::new("kimi-code", "org-1", "subscription").with_model("kimi-k2"),
        UsageQuotaScopeKey::new("kimi-code", "org-1", "subscription").with_key("key-a"),
    ] {
        assert!(!base.shares_allowance(&other));
        assert_ne!(base.dedup_key(), other.dedup_key());
    }
    // Length-prefixing keeps ("ab","c") distinct from ("a","bc").
    let left = UsageQuotaScopeKey::new("ab", "c", "s");
    let right = UsageQuotaScopeKey::new("a", "bc", "s");
    assert_ne!(left.dedup_key(), right.dedup_key());
    // Missing scope detail never equals an empty string.
    let missing = UsageQuotaScopeKey::new("s", "b", "c");
    let empty_key = UsageQuotaScopeKey::new("s", "b", "c").with_key("");
    assert!(!missing.shares_allowance(&empty_key));
    assert_ne!(missing.dedup_key(), empty_key.dedup_key());
}

#[test]
fn independent_key_caps_never_merge() {
    let key_a = UsageQuotaScopeKey::new("openai", "org-1", "key").with_key("key-a");
    let key_b = UsageQuotaScopeKey::new("openai", "org-1", "key").with_key("key-b");
    assert!(!key_a.shares_allowance(&key_b));
    assert_ne!(key_a.dedup_key(), key_b.dedup_key());
    // Same key identity under one billing subject shares one observation.
    let key_a_alias = UsageQuotaScopeKey::new("openai", "org-1", "key").with_key("key-a");
    assert!(key_a.shares_allowance(&key_a_alias));
    assert_eq!(key_a.dedup_key(), key_a_alias.dedup_key());
    // Unscoped subscription allowance never merges with a key cap.
    let subscription = UsageQuotaScopeKey::new("openai", "org-1", "subscription");
    assert!(!subscription.shares_allowance(&key_a));
}

#[test]
fn canonical_projection_v1_forty_account_fixture_stays_below_transport_margin() {
    let mut projection: UsageProjectionV1 = serde_json::from_str(include_str!(
        "../../../../../services/jackin-usage/tests/fixtures/contracts/usage-projection-v1-current.json"
    ))
    .unwrap();
    let seed = projection.providers[0].accounts[0].clone();
    projection.providers[0].accounts = (0..40)
        .map(|rank| {
            let mut account = seed.clone();
            account.rank = rank;
            account.canonical_account_id = format!("account-{rank:02}");
            account.display_label = format!("account-{rank:02}@example.test");
            account
        })
        .collect();
    projection.validate().unwrap();
    let encoded = serde_json::to_vec(&projection).unwrap();
    assert!(
        encoded.len() < USAGE_BROKER_MAX_FRAME_BYTES * 3 / 4,
        "40-account fixture is {} bytes",
        encoded.len()
    );
}
