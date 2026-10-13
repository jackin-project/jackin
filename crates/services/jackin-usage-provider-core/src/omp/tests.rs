// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::super::{StatusSlot, bucket, with_status_slot};
use super::{
    OMP_AUTH_BROKER_ACCOUNT_POOL_FILE, UsageConfidence, UsageSnapshotStatus, UsageSource,
    omp_attribute, omp_attributed_view, omp_broker_pool_is_authorization, omp_pool_routing_buckets,
};

/// Sanitized broker pool-routing fixture: routing state only, no secrets.
const OMP_POOL_ROUTING_FIXTURE: &str = r#"{
    "pools": [
        {"provider": "anthropic", "filter": "pro", "in_flight": 2, "served": 41},
        {"provider": "openai", "filter": "default", "in_flight": 0, "served": 7}
    ]
}"#;

fn session_bucket(remaining: u8) -> super::QuotaBucketView {
    with_status_slot(
        bucket(
            "Session",
            Some("11% used".to_owned()),
            None,
            Some(remaining),
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ),
        Some(StatusSlot::Session),
    )
}

#[test]
fn attribution_preserves_underlying_buckets() {
    let buckets = vec![session_bucket(89)];
    let attribution = omp_attribute("Anthropic", "operator@example.com", buckets.clone());
    assert_eq!(attribution.provider, "Anthropic");
    assert_eq!(attribution.account_label, "operator@example.com");
    assert_eq!(attribution.buckets, buckets);
}

#[test]
fn pool_routing_yields_no_budget() {
    let routing: serde_json::Value =
        serde_json::from_str(OMP_POOL_ROUTING_FIXTURE).expect("fixture parses");
    assert!(omp_pool_routing_buckets(&routing).is_empty());
}

#[test]
fn broker_pool_is_not_authorization() {
    assert!(!omp_broker_pool_is_authorization());
    assert_eq!(
        OMP_AUTH_BROKER_ACCOUNT_POOL_FILE,
        "OMP_AUTH_BROKER_ACCOUNT_POOL_FILE"
    );
}

#[test]
fn view_attributes_underlying_account() {
    let attribution = omp_attribute(
        "Anthropic",
        "operator@example.com",
        vec![session_bucket(89)],
    );
    let view = omp_attributed_view("omp", &attribution, 1_780_000_000);
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.source, UsageSource::ProviderApi);
    assert_eq!(view.confidence, UsageConfidence::Authoritative);
    assert_eq!(view.focused_agent.as_deref(), Some("omp"));
    assert_eq!(view.focused_provider.as_deref(), Some("Anthropic"));
    assert_eq!(view.account.provider_label, "Anthropic");
    assert_eq!(view.account.account_label, "operator@example.com");
    assert_eq!(
        view.account.credential_origin.as_deref(),
        Some("omp provider entry")
    );
    assert_eq!(view.buckets.len(), 1);
    assert_eq!(view.status_bar_label, "Session 89%");
    assert!(view.last_error.is_none());
}

#[test]
fn view_without_buckets_is_unavailable() {
    let attribution = omp_attribute("Anthropic", "operator@example.com", Vec::new());
    let view = omp_attributed_view("omp", &attribution, 1_780_000_000);
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(view.source, UsageSource::None);
    assert_eq!(view.confidence, UsageConfidence::None);
    assert!(view.buckets.is_empty());
    assert_eq!(view.status_bar_label, "usage unavailable");
    assert!(view.last_error.is_some());
}
