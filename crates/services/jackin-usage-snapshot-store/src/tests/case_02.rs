// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    CountQuota, CountQuotaPeriod, CountQuotaProvenance, CountQuotaUnit, Money,
    PercentObservationOrigin,
};

#[test]
fn all_provider_snapshots_round_trip_from_turso_to_usage_overlay_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    let providers = [
        (
            "Codex",
            "OpenAI",
            "codex@example.com",
            Some("Pro 20x"),
            "Session",
            37,
        ),
        (
            "Claude",
            "Anthropic",
            "claude@example.com",
            Some("Max"),
            "Weekly",
            42,
        ),
        (
            "Amp",
            "Amp",
            "amp@example.com",
            Some("Amp Free"),
            "Amp Free",
            55,
        ),
        ("Grok Build", "xAI", "local Grok auth", None, "Credits", 61),
        (
            "GLM / Z.AI",
            "Z.AI",
            "zai@example.com",
            Some("GLM Coding"),
            "Tokens",
            72,
        ),
        (
            "Kimi",
            "Kimi",
            "kimi@example.com",
            Some("K2"),
            "5-hour rate limit",
            83,
        ),
        (
            "MiniMax",
            "MiniMax",
            "minimax@example.com",
            Some("MiniMax Pro"),
            "MiniMax Text Coding plan",
            94,
        ),
    ];

    for (provider, _tab_label, account, plan, bucket, remaining) in providers {
        store_usage_snapshot(
            &db,
            &provider_usage_view(provider, account, plan, bucket, remaining, now - 120),
        )
        .expect("store provider snapshot");
    }

    for (provider, tab_label, account, plan, bucket, remaining) in providers {
        let view = focused_usage_view(&db, Some("codex"), Some(tab_label), now)
            .expect("read focused usage")
            .expect("stored provider usage");
        assert_eq!(view.account.provider_label, provider);
        assert_eq!(view.account.account_label, account);
        assert_eq!(view.account.plan_label.as_deref(), plan);
        assert_eq!(view.buckets.len(), 1);
        assert_eq!(view.buckets[0].label, bucket);
        assert_eq!(
            view.buckets[0].remaining_percent.unwrap().get(),
            f64::from(remaining)
        );
        assert_eq!(view.updated_label, "Updated 2m ago");
        assert_eq!(view.tabs.len(), 7);
        let mut ids: Vec<&str> = view.tabs.iter().map(|tab| tab.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 7);
        assert_eq!(
            view.tabs
                .iter()
                .map(|tab| tab.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Amp · amp@example.com",
                "Claude · claude@example.com",
                "Codex · codex@example.com",
                "GLM / Z.AI · zai@example.com",
                "Grok Build · local Grok auth",
                "Kimi · kimi@example.com",
                "MiniMax · minimax@example.com",
            ]
        );
        let tab = view
            .tabs
            .iter()
            .find(|tab| tab.account_label == account)
            .expect("account tab");
        assert_eq!(tab.id, account_key_hash(provider, account));

        let _unused = (view, account, plan, bucket, remaining, tab_label);
    }
}

#[test]
fn same_provider_accounts_keep_distinct_store_tabs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let now = 1_781_185_680;
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Claude",
            "a@example.com",
            Some("Max"),
            "Session",
            40,
            now - 120,
        ),
    )
    .expect("store claude-a snapshot");
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Claude",
            "b@example.com",
            Some("Max 20x"),
            "Session",
            60,
            now - 60,
        ),
    )
    .expect("store claude-b snapshot");
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Codex",
            "codex@example.com",
            Some("Pro 20x"),
            "Session",
            37,
            now - 30,
        ),
    )
    .expect("store codex snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Anthropic"), now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(view.tabs.len(), 3);
    let claude: Vec<_> = view
        .tabs
        .iter()
        .filter(|tab| tab.account_label == "a@example.com" || tab.account_label == "b@example.com")
        .collect();
    assert_eq!(claude.len(), 2);
    assert_ne!(claude[0].id, claude[1].id);
    assert_ne!(claude[0].label, claude[1].label);
    assert_eq!(
        claude[0].id,
        account_key_hash("Claude", claude[0].account_label.as_str())
    );
    assert_eq!(
        claude[1].id,
        account_key_hash("Claude", claude[1].account_label.as_str())
    );
}

#[test]
fn sqlite_typed_bucket_round_trips_exact_count_money_and_fractional_evidence() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");
    let mut view = usage_view();
    let requests = &mut view.buckets[0];
    requests.used_percent = None;
    requests.used_percent_origin = None;
    requests.remaining_percent = None;
    requests.remaining_percent_origin = None;
    requests.count_quota = Some(CountQuota::new(
        Some(u64::MAX),
        Some(u64::MAX),
        Some(0),
        CountQuotaUnit::Requests,
        CountQuotaPeriod::UtcDaily,
        CountQuotaProvenance::ProviderReported,
    ));
    requests.provider_quota_state = Some(jackin_protocol::control::ProviderQuotaState::Exhausted);

    let money = &mut view.buckets[1];
    let adjacent_used = f64::from_bits(43.375_f64.to_bits() + 1);
    let adjacent_remaining = f64::from_bits(57.125_f64.to_bits() + 1);
    money.used_percent =
        Some(jackin_protocol::control::CanonicalUsedPercent::new(adjacent_used).unwrap());
    money.used_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    money.remaining_percent =
        Some(jackin_protocol::control::CanonicalRemainingPercent::new(adjacent_remaining).unwrap());
    money.remaining_percent_origin = Some(PercentObservationOrigin::ProviderReported);
    money.used_money = Some(Money::new(i64::MIN, "USD", 4));
    money.limit_money = Some(Money::new(i64::MAX, "USD", 7));

    let mut unlimited = view.buckets[0].clone();
    unlimited.label = "Unlimited".to_owned();
    unlimited.used_label = None;
    unlimited.limit_label = None;
    unlimited.used_percent = None;
    unlimited.used_percent_origin = None;
    unlimited.remaining_percent = None;
    unlimited.remaining_percent_origin = None;
    unlimited.count_quota = None;
    unlimited.provider_quota_state = Some(jackin_protocol::control::ProviderQuotaState::Unlimited);
    unlimited.used_money = None;
    unlimited.limit_money = None;
    view.buckets.push(unlimited);

    store_usage_snapshot(&db, &view).expect("store exact typed buckets");
    block_on_store(close_cached_store(&db)).expect("close store before durable reopen");
    let rows = stored_account_snapshots(&db).expect("read exact typed buckets");
    let requests = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .unwrap();
    assert_eq!(
        requests.quota_bucket.count_quota.unwrap().used,
        Some(u64::MAX)
    );
    assert_eq!(
        requests.quota_bucket.provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Exhausted)
    );
    let money = rows
        .iter()
        .find(|row| row.window_kind == "Credits")
        .unwrap();
    assert_eq!(
        money.quota_bucket.used_percent.unwrap().get().to_bits(),
        adjacent_used.to_bits()
    );
    assert_eq!(
        money
            .quota_bucket
            .remaining_percent
            .unwrap()
            .get()
            .to_bits(),
        adjacent_remaining.to_bits()
    );
    assert_eq!(
        money.quota_bucket.used_money.as_ref().unwrap().amount_minor,
        i64::MIN
    );
    assert_eq!(
        money
            .quota_bucket
            .limit_money
            .as_ref()
            .unwrap()
            .amount_minor,
        i64::MAX
    );
    let unlimited = rows
        .iter()
        .find(|row| row.window_kind == "Unlimited")
        .unwrap();
    assert_eq!(
        unlimited.quota_bucket.provider_quota_state,
        Some(jackin_protocol::control::ProviderQuotaState::Unlimited)
    );
}

#[test]
fn usage_snapshot_store_records_schema_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("snapshots.db");

    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    assert_eq!(
        schema_version(&db).expect("schema version").as_deref(),
        Some("6")
    );
}
