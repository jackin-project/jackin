// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn openrouter_cache_preserves_exact_capability_and_last_good_rows_on_error() {
    let capability = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: "account-openrouter".to_owned(),
        surface_id: "openrouter".to_owned(),
    };
    assert!(capability_matches_surface(
        "opencode",
        Some("OpenRouter"),
        &capability
    ));

    let target = UsageRefreshTarget {
        agent: "opencode".to_owned(),
        provider: Some("OpenRouter".to_owned()),
        capability: capability.clone(),
    };
    let mut view = provider_credential_snapshot("openrouter", "OPENROUTER_API_KEY", "");
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;

    let mut cache = UsageCache::default();
    cache.insert_snapshot_for_capability_for_test(
        "opencode",
        Some("OpenRouter"),
        &capability,
        view,
    );
    cache.adopt_broker_error(
        &target,
        &jackin_protocol::usage_broker::UsageCoordinationError {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::ProviderUnavailable,
            message: "OpenRouter key request failed".to_owned(),
        },
    );

    let adopted = cache.focused_snapshot_for_capability(
        Some("opencode"),
        Some("OpenRouter"),
        Some(&capability),
    );
    assert_eq!(adopted.status, UsageSnapshotStatus::Stale);
    assert_eq!(adopted.account.provider_label, "OpenRouter");
    assert_eq!(adopted.buckets[0].label, "Usage");
    assert_eq!(
        adopted.last_error.as_deref(),
        Some("OpenRouter key request failed")
    );

    let wrong_surface = jackin_protocol::usage_broker::UsageAccountCapability {
        account_id: capability.account_id.clone(),
        surface_id: "opencode".to_owned(),
    };
    assert!(!capability_matches_surface(
        "opencode",
        Some("OpenRouter"),
        &wrong_surface
    ));
    assert_eq!(
        cache
            .focused_snapshot_for_capability(
                Some("opencode"),
                Some("OpenRouter"),
                Some(&wrong_surface),
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

#[test]
fn usage_cache_adopts_broker_generations_by_account_capability() {
    let target = UsageRefreshTarget {
        agent: "codex".to_owned(),
        provider: Some("OpenAI".to_owned()),
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "account-a".to_owned(),
            surface_id: "codex".to_owned(),
        },
    };
    let generation = |account_id: &str, account_label: &str| {
        let mut view = codex_cached_usage_view();
        view.account.account_label = account_label.to_owned();
        jackin_protocol::usage_broker::UsageGenerationView {
            capability: jackin_protocol::usage_broker::UsageAccountCapability {
                account_id: account_id.to_owned(),
                surface_id: "codex".to_owned(),
            },
            generation: 1,
            phase: jackin_protocol::usage_broker::UsageRefreshPhase::Completed,
            snapshot: Some(view),
            error: None,
            retry_at_epoch: None,
        }
    };

    let mut cache = UsageCache::default();
    cache.adopt_broker_generation(&target, &generation("account-a", "personal@example.test"));
    let other_target = UsageRefreshTarget {
        capability: jackin_protocol::usage_broker::UsageAccountCapability {
            account_id: "account-b".to_owned(),
            surface_id: "codex".to_owned(),
        },
        ..target.clone()
    };
    cache.adopt_broker_generation(&other_target, &generation("account-b", "work@example.test"));

    assert_eq!(cache.snapshots.len(), 2);
    assert_eq!(cache.account_snapshot_views().len(), 2);
    cache.adopt_broker_error(
        &target,
        &jackin_protocol::usage_broker::UsageCoordinationError {
            kind: jackin_protocol::usage_broker::UsageCoordinationErrorKind::ProviderUnavailable,
            message: "provider unavailable".to_owned(),
        },
    );
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
