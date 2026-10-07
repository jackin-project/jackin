// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn failed_refresh_preserves_last_fresh_quota_rows_as_stale_cache() {
    let mut cached = FocusedUsageView::unavailable("seed", 123);
    cached.status = UsageSnapshotStatus::Fresh;
    cached.confidence = UsageConfidence::Authoritative;
    cached.account = FocusedAccountHeader {
        provider_label: "OpenAI / Codex".to_owned(),
        account_label: "alexey@example.com".to_owned(),
        username: None,
        plan_label: Some("Pro 20x".to_owned()),
        credential_origin: None,
    };
    cached.buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Weekly".to_owned(),
        used_label: Some("90% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(10),
        reset_label: Some("Resets in 3h 52m".to_owned()),
        resets_at: None,
        status_slot: Some(StatusSlot::Weekly),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
    }];

    for failed_status in [
        UsageSnapshotStatus::Stale,
        UsageSnapshotStatus::NeedsLogin,
        UsageSnapshotStatus::Error,
    ] {
        let mut view = FocusedUsageView::unavailable("seed", 124);
        view.focused_agent = Some("codex".to_owned());
        view.focused_provider = Some("Codex".to_owned());
        view.status = failed_status;
        view.account = FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: None,
            credential_origin: None,
        };
        view.last_error = Some("Codex provider usage unavailable".to_owned());

        preserve_cached_quota_on_failed_refresh(&mut view, &cached);

        assert_eq!(view.status, UsageSnapshotStatus::Stale);
        assert_eq!(view.source, UsageSource::Cache);
        assert_eq!(view.confidence, UsageConfidence::Authoritative);
        assert_eq!(view.buckets.len(), 1);
        assert_eq!(view.buckets[0].status, UsageSnapshotStatus::Stale);
        assert_eq!(view.account.plan_label.as_deref(), Some("Pro 20x"));
        assert_eq!(view.status_bar_label, "Weekly 10%");
        assert!(
            view.last_error
                .as_deref()
                .is_some_and(|error| error.contains("showing last cached quota"))
        );
    }
}

#[test]
fn broker_client_failure_preserves_last_good_quota() {
    let target = UsageRefreshTarget {
        agent: "claude".to_owned(),
        provider: Some("Claude".to_owned()),
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "account-claude".to_owned(),
            surface_id: "claude".to_owned(),
        },
    };
    let mut cached = FocusedUsageView::unavailable("seed", 123);
    cached.status = UsageSnapshotStatus::Fresh;
    cached.buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
        label: "Weekly".to_owned(),
        used_label: Some("36% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(64),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Weekly),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
    }];
    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_capability_for_test(
        "claude",
        Some("Claude"),
        &target.capability,
        cached,
    );

    cache.adopt_broker_error(
        &target,
        &jackin_protocol::usage_broker::UsageCoordinationError {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::Unavailable,
            message: "usage broker is unavailable".to_owned(),
        },
    );

    let adopted = cache.focused_snapshot_for_capability(
        Some("claude"),
        Some("Claude"),
        Some(&target.capability),
    );
    assert_eq!(adopted.status, UsageSnapshotStatus::Stale);
    assert_eq!(adopted.buckets[0].remaining_percent, Some(64));
    assert_eq!(adopted.buckets[0].status, UsageSnapshotStatus::Stale);
    assert_eq!(
        cache
            .snapshots
            .values()
            .next()
            .map(|cached| cached.view.updated_label.as_str()),
        Some("Stale")
    );
    assert_eq!(
        adopted.last_error.as_deref(),
        Some("usage broker is unavailable")
    );
}
