
use crate::tui::components::dialog::Dialog;

use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, UsageConfidence, UsageSnapshotStatus,
    UsageSource,
};

use super::*;

fn usage_view() -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("OpenAI".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "Codex".to_owned(),
            account_label: "alexey@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: None,
        },
        buckets: vec![
            QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets in 1h".to_owned()),
                resets_at: None,
                status_slot: None,
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
            },
            QuotaBucketView {
                label: "Credits".to_owned(),
                used_label: None,
                limit_label: None,
                remaining_percent: None,
                reset_label: None,
                resets_at: None,
                status_slot: None,
                pace_label: Some("ACP billing unavailable".to_owned()),
                status: UsageSnapshotStatus::Unsupported,
            },
        ],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::Cli,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_781_185_560,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

fn provider_usage_view(
    provider: &str,
    account: &str,
    plan: Option<&str>,
    bucket: &str,
    remaining: u8,
    fetched_at_epoch: i64,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some(provider.to_owned()),
        account: FocusedAccountHeader {
            provider_label: provider.to_owned(),
            account_label: account.to_owned(),
            username: None,
            plan_label: plan.map(str::to_owned),
            credential_origin: None,
        },
        buckets: vec![QuotaBucketView {
            label: bucket.to_owned(),
            used_label: Some(format!("{}% used", 100_u8.saturating_sub(remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(remaining),
            reset_label: Some("Resets at 15:00 UTC".to_owned()),
            resets_at: None,
            status_slot: None,
            pace_label: Some("On pace".to_owned()),
            status: UsageSnapshotStatus::Fresh,
        }],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch,
        updated_label: "Updated just now".to_owned(),
        status_bar_label: format!("{bucket} {remaining}%"),
        tabs: Vec::new(),
        last_error: None,
    }
}

#[test]
fn account_snapshot_rows_are_persisted_and_upserted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");

    store_usage_snapshot(&db, &usage_view()).expect("store first snapshot");
    let mut changed = usage_view();
    changed.buckets[0].remaining_percent = Some(25);
    changed.fetched_at_epoch += 60;
    store_usage_snapshot(&db, &changed).expect("store updated snapshot");

    let rows = stored_account_snapshots(&db).expect("read snapshots");
    assert_eq!(rows.len(), 2);
    let session = rows
        .iter()
        .find(|row| row.window_kind == "Session")
        .expect("session row");
    assert_eq!(session.provider, "Codex");
    assert!(session.account_key_hash.starts_with("sha256:"));
    assert_eq!(session.source, "cli");
    assert_eq!(session.confidence, "authoritative");
    assert_eq!(session.used_amount, Some(75));
    assert_eq!(session.used_unit.as_deref(), Some("percent"));
    assert_eq!(session.limit_amount, Some(100));
    assert_eq!(session.status, "fresh");
    assert_eq!(session.fetched_at, 1_781_185_620);
    assert_eq!(session.remaining_percent, Some(25));
    assert_eq!(session.used_label.as_deref(), Some("63% used"));
    assert_eq!(session.limit_label.as_deref(), Some("100%"));
    assert_eq!(session.plan_label.as_deref(), Some("Pro 20x"));
}

#[test]
fn focused_usage_view_rebuilds_snapshot_from_account_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Codex"), 1_781_185_590)
        .expect("read focused usage")
        .expect("stored usage view");

    assert_eq!(view.focused_agent.as_deref(), Some("codex"));
    assert_eq!(view.focused_provider.as_deref(), Some("OpenAI"));
    assert_eq!(view.account.provider_label, "Codex");
    assert_eq!(view.account.account_label, "alexey@example.com");
    assert_eq!(view.account.plan_label.as_deref(), Some("Pro 20x"));
    assert_eq!(view.buckets.len(), 2);
    assert_eq!(view.buckets[0].label, "Session");
    assert_eq!(view.buckets[0].remaining_percent, Some(37));
    assert_eq!(view.buckets[1].label, "Credits");
    // Restored buckets carry no status-bar slot: the headline is persisted as
    // `status_bar_label` and read directly, never recomputed from the restored
    // (untagged) buckets. Locks that contract so a future change recomputing
    // the headline from buckets — which would blank every cached headline —
    // fails loudly here.
    assert!(
        view.buckets
            .iter()
            .all(|bucket| bucket.status_slot.is_none())
    );
    assert_eq!(view.updated_label, "Updated just now");
    assert_eq!(view.status_bar_label, "Codex Session: 63% used · 37% left");
}

#[test]
fn focused_usage_view_ticks_relative_updated_label_from_fetch_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Codex"), 1_781_185_680)
        .expect("read focused usage")
        .expect("stored usage view");

    assert_eq!(view.updated_label, "Updated 2m ago");
}

#[test]
fn focused_usage_view_resolves_provider_from_agent_when_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");
    let now = 1_781_185_680;
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Codex",
            "codex@example.com",
            Some("Pro 20x"),
            "Session",
            37,
            now,
        ),
    )
    .expect("store codex snapshot");
    store_usage_snapshot(
        &db,
        &provider_usage_view(
            "Amp",
            "amp@example.com",
            Some("Amp Free"),
            "Amp Free",
            9,
            now,
        ),
    )
    .expect("store amp snapshot");

    let view = focused_usage_view(&db, Some("amp"), None, now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(view.focused_agent.as_deref(), Some("amp"));
    assert_eq!(view.focused_provider.as_deref(), Some("Amp"));
    assert_eq!(view.account.provider_label, "Amp");
    assert_eq!(view.account.account_label, "amp@example.com");
    assert_eq!(view.buckets[0].label, "Amp Free");
    assert_eq!(view.buckets[0].remaining_percent, Some(9));
}

#[test]
fn focused_usage_view_without_resolved_provider_does_not_match_all() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");
    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    let view = focused_usage_view(&db, Some("unknown-agent"), None, 1_781_185_680)
        .expect("read focused usage");

    assert!(view.is_none());
}

#[test]
fn focused_usage_view_sorts_provider_buckets_canonically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");
    let now = 1_781_185_680;
    let mut view = provider_usage_view(
        "GLM / Z.AI",
        "zai@example.com",
        Some("Coding Pro"),
        "5-hour",
        100,
        now,
    );
    let base_bucket = view.buckets[0].clone();
    view.buckets.extend([
        QuotaBucketView {
            label: "MCP".to_owned(),
            remaining_percent: Some(100),
            pace_label: Some("0 / 100 (100 remaining)".to_owned()),
            ..base_bucket.clone()
        },
        QuotaBucketView {
            label: "Tokens".to_owned(),
            remaining_percent: Some(99),
            ..base_bucket
        },
    ]);
    store_usage_snapshot(&db, &view).expect("store snapshot");

    let view = focused_usage_view(&db, Some("codex"), Some("Z.AI"), now)
        .expect("read focused usage")
        .expect("stored provider usage");

    assert_eq!(
        view.buckets
            .iter()
            .map(|bucket| bucket.label.as_str())
            .collect::<Vec<_>>(),
        vec!["5-hour", "Tokens", "MCP"]
    );
}

#[test]
fn all_provider_snapshots_round_trip_from_turso_to_usage_overlay_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");
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
        assert_eq!(view.buckets[0].remaining_percent, Some(remaining));
        assert_eq!(view.updated_label, "Updated 2m ago");
        assert_eq!(view.tabs.len(), 7);

        let state = Dialog::new_usage(view).usage_state().expect("usage state");
        let rows = state.rows();
        assert!(
            rows.iter()
                .any(|row| row.label() == "Header" && row.value() == tab_label),
            "provider header row missing for {provider}: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|row| row.label() == bucket && row.value().contains("left")),
            "bucket row missing for {provider}/{bucket}: {rows:?}"
        );
    }
}

#[test]
fn telemetry_store_records_schema_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("usage.db");

    store_usage_snapshot(&db, &usage_view()).expect("store snapshot");

    assert_eq!(
        schema_version(&db).expect("schema version").as_deref(),
        Some("4")
    );
}
