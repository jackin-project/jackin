// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn status_bar_label_uses_stale_cached_percentages() {
    let buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Session".to_owned(),
        used_label: Some("99% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(1),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Stale,
    }];

    assert_eq!(
        status_bar_label(
            UsageSurface::Claude,
            "alexey@example.com",
            UsageSnapshotStatus::Stale,
            &buckets
        ),
        "Session 1%"
    );
}

#[test]
fn status_bar_label_drops_tagged_bucket_that_failed() {
    // A Session-tagged bucket whose own status is not Fresh/Stale (e.g. the
    // window errored) must not surface its percentage as if it were live;
    // the headline falls through to the snapshot-level status label.
    let buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Session".to_owned(),
        used_label: Some("50% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(50),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Error,
    }];

    assert_eq!(
        status_bar_label(
            UsageSurface::Claude,
            "alexey@example.com",
            UsageSnapshotStatus::Error,
            &buckets
        ),
        "error"
    );
}

#[test]
fn status_bar_label_uses_amp_daily_only() {
    let buckets = vec![
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Amp Free".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(48),
            reset_label: Some("Resets daily".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Daily),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
        },
        QuotaBucketView {
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::default(),
            label: "Individual credits".to_owned(),
            used_label: None,
            limit_label: Some("$4.76".to_owned()),
            remaining_percent: None,
            reset_label: None,
            resets_at: None,
            status_slot: None,
            pace_label: Some("Individual credits: $4.76".to_owned()),
            status: UsageSnapshotStatus::Fresh,
        },
    ];

    // Daily is the only glance; credits stay detail-only.
    assert_eq!(
        status_bar_label(
            UsageSurface::Amp,
            "alexey@example.com",
            UsageSnapshotStatus::Fresh,
            &buckets
        ),
        "Free 48%"
    );
}

#[test]
fn status_bar_label_uses_stale_amp_cache() {
    let buckets = vec![QuotaBucketView {
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::default(),
        label: "Amp Free".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(9),
        reset_label: Some("Resets daily".to_owned()),
        resets_at: None,
        status_slot: Some(StatusSlot::Daily),
        pace_label: None,
        status: UsageSnapshotStatus::Stale,
    }];

    assert_eq!(
        status_bar_label(
            UsageSurface::Amp,
            "alexey@example.com",
            UsageSnapshotStatus::Stale,
            &buckets
        ),
        "Free 9%"
    );
}

#[test]
fn usage_cache_key_canonicalizes_provider_aliases() {
    assert_eq!(
        canonical_usage_cache_key("claude", Some("Anthropic")),
        canonical_usage_cache_key("claude", Some("Anthropic / Claude"))
    );
    assert_eq!(
        canonical_usage_cache_key("codex", Some("OpenAI")),
        canonical_usage_cache_key("codex", Some("OpenAI / Codex"))
    );
    assert_eq!(
        canonical_usage_cache_key("claude", Some("Z.AI")),
        canonical_usage_cache_key("glm", Some("GLM / Z.AI"))
    );
    assert_eq!(
        canonical_usage_cache_key("opencode", Some("OpenRouter")),
        "OpenRouter"
    );
    assert_ne!(
        canonical_usage_cache_key("claude", Some("Anthropic")),
        canonical_usage_cache_key("claude", Some("Z.AI"))
    );
}

#[test]
fn usage_cache_keeps_account_snapshots_isolated_across_one_provider_target() {
    let personal = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "account-personal".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let work = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "account-work".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let mut first = codex_cached_usage_view();
    first.account.account_label = "personal@example.test".to_owned();
    let mut second = codex_cached_usage_view();
    second.account.account_label = "work@example.test".to_owned();

    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_capability_for_test("codex", Some("OpenAI"), &personal, first);
    cache.insert_snapshot_for_capability_for_test("codex", Some("OpenAI"), &work, second);

    assert_eq!(cache.snapshots.len(), 2);
    let rows = cache.account_snapshot_views();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .any(|row| row.account_label == "personal@example.test")
    );
    assert!(
        rows.iter()
            .any(|row| row.account_label == "work@example.test")
    );

    cache.adopt_broker_error(
        &UsageRefreshTarget {
            agent: "codex".to_owned(),
            provider: Some("OpenAI".to_owned()),
            capability: personal.clone(),
        },
        &jackin_protocol::usage_broker::UsageCoordinationError {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::ProviderUnavailable,
            message: "provider unavailable".to_owned(),
        },
    );
    assert_eq!(cache.snapshots.len(), 2);
    assert_eq!(
        cache
            .snapshots
            .values()
            .find(|cached| cached.view.account.account_label == "personal@example.test")
            .map(|cached| cached.view.status),
        Some(UsageSnapshotStatus::Stale)
    );
    assert_eq!(
        cache
            .snapshots
            .values()
            .find(|cached| cached.view.account.account_label == "work@example.test")
            .map(|cached| cached.view.status),
        Some(UsageSnapshotStatus::Fresh)
    );
}

#[test]
fn usage_cache_rejects_provider_surface_capability_mismatch() {
    let capability = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "account-codex".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let target = UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("Claude".to_owned()),
        capability: capability.clone(),
    };
    let state = jackin_protocol::usage_broker::UsageGenerationView {
        capability,
        generation: 1,
        phase: jackin_protocol::usage_broker::UsageRefreshPhase::Completed,
        snapshot: Some(codex_cached_usage_view()),
        error: None,
        retry_at_epoch: None,
    };
    let mut cache = UsageCache::default();

    cache.adopt_broker_generation(&target, &state);
    assert!(cache.snapshots.is_empty());
    assert_eq!(
        cache
            .focused_snapshot_for_capability(
                Some("codex"),
                Some("Claude"),
                Some(&target.capability),
            )
            .status,
        UsageSnapshotStatus::Unavailable
    );
}

#[test]
fn empty_broker_error_snapshot_is_error_not_fresh() {
    let target = UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("OpenAI".to_owned()),
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "account-codex".to_owned(),
            surface_id: "codex".to_owned(),
        },
    };
    let mut empty = codex_cached_usage_view();
    empty.status = UsageSnapshotStatus::Fresh;
    empty.buckets.clear();
    let error = jackin_protocol::usage_broker::UsageCoordinationError {
        kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::ProviderUnavailable,
        message: "fixture provider unavailable".to_owned(),
    };
    let state = jackin_protocol::usage_broker::UsageGenerationView {
        capability: target.capability.clone(),
        generation: 1,
        phase: jackin_protocol::usage_broker::UsageRefreshPhase::Completed,
        snapshot: Some(empty.clone()),
        error: Some(error.clone()),
        retry_at_epoch: None,
    };
    let mut cache = UsageCache::default();
    cache.adopt_broker_generation(&target, &state);
    assert_eq!(
        cache
            .focused_snapshot_for_capability(
                Some("codex"),
                Some("OpenAI"),
                Some(&target.capability),
            )
            .status,
        UsageSnapshotStatus::Error
    );

    let mut second = UsageCache::default();
    second.insert_snapshot_for_capability_for_test(
        "codex",
        Some("OpenAI"),
        &target.capability,
        empty,
    );
    second.adopt_broker_error(&target, &error);
    assert_eq!(
        second
            .focused_snapshot_for_capability(
                Some("codex"),
                Some("OpenAI"),
                Some(&target.capability),
            )
            .status,
        UsageSnapshotStatus::Error
    );
}

#[test]
fn focused_usage_cache_selects_the_exact_account_capability() {
    let personal = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "account-personal".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let work = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "account-work".to_owned(),
        surface_id: "codex".to_owned(),
    };
    let mut personal_view = codex_cached_usage_view();
    personal_view.status_bar_label = "personal account".to_owned();
    personal_view.account.account_label = "same@example.test".to_owned();
    let mut work_view = codex_cached_usage_view();
    work_view.status_bar_label = "work account".to_owned();
    work_view.account.account_label = "same@example.test".to_owned();

    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_capability_for_test(
        "codex",
        Some("OpenAI"),
        &personal,
        personal_view,
    );
    cache.insert_snapshot_for_capability_for_test("codex", Some("OpenAI"), &work, work_view);

    assert_eq!(
        cache
            .focused_snapshot_for_capability(Some("codex"), Some("OpenAI"), Some(&personal))
            .status_bar_label,
        "personal account"
    );
    assert_eq!(
        cache
            .focused_snapshot_for_capability(Some("codex"), Some("OpenAI"), Some(&work))
            .status_bar_label,
        "work account"
    );
}

#[test]
fn usage_cache_isolates_provider_targets_that_share_one_agent_slug() {
    let mut zai = codex_cached_usage_view();
    zai.status_bar_label = "zai".to_owned();
    let mut minimax = codex_cached_usage_view();
    minimax.status_bar_label = "minimax".to_owned();

    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_test("codex", Some("Z.AI"), zai);
    cache.insert_snapshot_for_test("codex", Some("MiniMax"), minimax);

    assert_eq!(
        cache
            .focused_snapshot(Some("codex"), Some("Z.AI"))
            .status_bar_label,
        "zai"
    );
    assert_eq!(
        cache
            .focused_snapshot(Some("codex"), Some("MiniMax"))
            .status_bar_label,
        "minimax"
    );
}
