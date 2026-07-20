// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
};

fn open_runtime(dir: &Path) -> HostUsageRuntime {
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig::under_data_dir(dir))
        .expect("open");
    runtime
}

fn codex_fixture_view() -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: Some("codex".to_owned()),
        focused_provider: Some("Codex".to_owned()),
        account: FocusedAccountHeader {
            provider_label: "OpenAI / Codex".to_owned(),
            account_label: "codex@example.com".to_owned(),
            username: None,
            plan_label: Some("Pro 20x".to_owned()),
            credential_origin: Some("OAuth · ~/.codex/auth.json".to_owned()),
        },
        buckets: vec![
            QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets in 2h".to_owned()),
                resets_at: Some(1_700_000_000),
                status_slot: Some(StatusSlot::Session),
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
                used_money: None,
                limit_money: None,
                severity: UsageSeverity::Normal,
            },
            QuotaBucketView {
                label: "Weekly".to_owned(),
                used_label: Some("40% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(60),
                reset_label: Some("Resets in 3d".to_owned()),
                resets_at: Some(1_700_200_000),
                status_slot: Some(StatusSlot::Weekly),
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
                used_money: None,
                limit_money: None,
                severity: UsageSeverity::Normal,
            },
        ],
        status: UsageSnapshotStatus::Fresh,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_699_000_000,
        updated_label: "just now".to_owned(),
        status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
        tabs: Vec::new(),
        last_error: None,
    }
}

#[test]
fn host_surfaces_cover_agent_all_plus_routed_providers() {
    let agent_ids: HashSet<_> = Agent::ALL
        .iter()
        .map(|agent| HostSurfaceId::from_agent(*agent).id())
        .collect();
    for id in ["claude", "codex", "amp", "kimi", "opencode", "grok"] {
        assert!(agent_ids.contains(id), "missing agent surface {id}");
    }
    assert!(HostSurfaceId::from_id("zai").is_some());
    assert!(HostSurfaceId::from_id("minimax").is_some());
    assert!(HostSurfaceId::from_id("cursor").is_none());
    assert_eq!(HostSurfaceId::ALL.len(), 8);
}

#[test]
fn fixture_snapshot_matches_capsule_view_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let fixture = codex_fixture_view();
    runtime
        .inject_snapshot("codex", fixture.clone())
        .expect("inject");
    let view = runtime.snapshot("codex").expect("snapshot");
    assert_eq!(view.status_bar_label, fixture.status_bar_label);
    assert_eq!(view.buckets.len(), fixture.buckets.len());
    assert_eq!(
        view.buckets[0].remaining_percent,
        fixture.buckets[0].remaining_percent
    );
    assert_eq!(view.buckets[0].resets_at, fixture.buckets[0].resets_at);
    assert_eq!(view.status, UsageSnapshotStatus::Fresh);
    assert_eq!(view.account.account_label, "codex@example.com");
    assert_eq!(
        runtime.status_bar_label("codex").expect("label"),
        Some(fixture.status_bar_label)
    );
}

#[test]
fn unavailable_and_refreshing_never_invent_percent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // No inject → refreshing (focused agent path with empty cache).
    let refreshing = runtime.snapshot("claude").expect("snapshot");
    assert_eq!(refreshing.status_bar_label, "refreshing");
    assert!(
        refreshing
            .buckets
            .iter()
            .all(|bucket| bucket.remaining_percent.is_none()),
        "refreshing must not invent remaining_percent"
    );

    let unavailable = FocusedUsageView::unavailable("missing credentials", 42);
    runtime
        .inject_snapshot("claude", unavailable)
        .expect("inject");
    let view = runtime.snapshot("claude").expect("snapshot");
    assert_eq!(view.status, UsageSnapshotStatus::Unavailable);
    assert!(view.buckets.is_empty());
    assert_eq!(view.status_bar_label, "usage unavailable");
    assert!(
        !view.status_bar_label.chars().any(|c| c.is_ascii_digit()),
        "unavailable headline must not invent numbers"
    );
}

#[test]
fn disable_surface_removes_from_list_and_blocks_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime.set_enabled("claude", false).expect("disable");
    let listed = runtime.list_surfaces().expect("list");
    let claude = listed
        .iter()
        .find(|row| row.id == "claude")
        .expect("claude row");
    assert!(!claude.enabled);
    drop(runtime.snapshot("claude").unwrap_err());
    assert_eq!(runtime.status_bar_label("claude").expect("label"), None);
}

#[test]
fn merged_bar_skips_disabled_surfaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        runtime
            .set_enabled(surface.id(), *surface == HostSurfaceId::Codex)
            .expect("enable set");
    }
    runtime
        .inject_snapshot("codex", codex_fixture_view())
        .expect("inject");
    let merged = runtime.merged_status_bar_label().expect("merged");
    assert!(merged.contains("Codex"));
    assert!(merged.contains("63%"));
    assert!(!merged.contains("Claude:"));
}

#[test]
fn money_bucket_preserved_in_host_snapshot() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let mut view = FocusedUsageView::unavailable("seed", 1);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.status_bar_label = "Session 10% · SGD 78 of 260".to_owned();
    view.buckets = vec![QuotaBucketView {
        label: "Spend".to_owned(),
        used_label: Some("SGD 78".to_owned()),
        limit_label: Some("SGD 260".to_owned()),
        remaining_percent: None,
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Spend),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: Some(Money::new(7800, "SGD", 2)),
        limit_money: Some(Money::new(26_000, "SGD", 2)),
        severity: UsageSeverity::Warn,
    }];
    runtime.inject_snapshot("claude", view).expect("inject");
    let got = runtime.snapshot("claude").expect("snapshot");
    let bucket = &got.buckets[0];
    assert_eq!(
        bucket.used_money.as_ref().map(|m| m.amount_minor),
        Some(7800)
    );
    assert_eq!(bucket.used_money.as_ref().map(|m| m.currency.as_str()), Some("SGD"));
    assert_eq!(bucket.severity, UsageSeverity::Warn);
}

#[test]
fn events_cursor_advances_and_bounds() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime.set_enabled("amp", false).expect("toggle");
    let batch = runtime.next_events(0, 10).expect("events");
    assert!(!batch.events.is_empty());
    assert!(batch.events.iter().any(|e| e.kind == "runtime_ready"));
    let next = runtime
        .next_events(batch.next_cursor, 10)
        .expect("empty tail");
    assert!(next.events.is_empty());
}

#[test]
fn credential_matrix_lists_all_host_surfaces() {
    let rows = host_credential_root_matrix();
    let surfaces: HashSet<_> = rows.iter().map(|row| row.surface).collect();
    for surface in HostSurfaceId::ALL {
        assert!(
            surfaces.contains(surface.id()),
            "matrix missing {}",
            surface.id()
        );
    }
}

#[test]
fn host_paths_under_data_dir() {
    let root = PathBuf::from("/tmp/jackin-data");
    assert_eq!(
        host_snapshot_store_path(&root),
        PathBuf::from("/tmp/jackin-data/usage-menu-bar/snapshots.db")
    );
    assert_eq!(
        host_accounts_path(&root),
        PathBuf::from("/tmp/jackin-data/usage-menu-bar/accounts.json")
    );
}
