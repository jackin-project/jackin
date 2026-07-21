// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::*;
use jackin_protocol::control::{
    FocusedAccountHeader, FocusedUsageView, QuotaBucketView, StatusSlot, UsageConfidence,
    UsageSeverity, UsageSnapshotStatus, UsageSource,
};
use jackin_usage::host::HostUsageRuntime;

fn open_bridge(dir: &std::path::Path) -> Arc<UsageMenuBarBridge> {
    let bridge = UsageMenuBarBridge::create();
    bridge
        .open_runtime(OpenConfig {
            data_dir: dir.display().to_string(),
            refresh_floor_secs: 120,
            enabled_surface_ids: vec!["codex".to_owned(), "claude".to_owned()],
        })
        .expect("open");
    bridge
}

#[test]
fn panic_probe_is_contained() {
    let bridge = UsageMenuBarBridge::create();
    let err = bridge.panic_probe().expect_err("panic");
    assert!(matches!(err, UsageBridgeError::ContainedPanic { .. }));
    // Still usable after containment.
    bridge.shutdown().expect("shutdown");
}

#[test]
fn fixture_snapshot_round_trip_via_bridge() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    // Inject through host runtime underneath for offline proof.
    {
        let mut guard = bridge.inner.lock().expect("lock");
        let view = FocusedUsageView {
            focused_agent: Some("codex".to_owned()),
            focused_provider: Some("Codex".to_owned()),
            account: FocusedAccountHeader {
                provider_label: "OpenAI / Codex".to_owned(),
                account_label: "codex@example.com".to_owned(),
                username: None,
                plan_label: Some("Pro 20x".to_owned()),
                credential_origin: None,
            },
            buckets: vec![QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("63% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(37),
                reset_label: Some("Resets in 2h".to_owned()),
                resets_at: Some(99),
                status_slot: Some(StatusSlot::Session),
                pace_label: None,
                status: UsageSnapshotStatus::Fresh,
                used_money: None,
                limit_money: None,
                severity: UsageSeverity::Normal,
            }],
            status: UsageSnapshotStatus::Fresh,
            source: UsageSource::ProviderApi,
            confidence: UsageConfidence::Authoritative,
            fetched_at_epoch: 1,
            updated_label: "just now".to_owned(),
            status_bar_label: "Codex Session: 63% used · 37% left".to_owned(),
            tabs: Vec::new(),
            last_error: None,
        };
        guard.inject_snapshot("codex", view).expect("inject");
    }
    let dto = bridge.snapshot("codex".to_owned()).expect("snapshot");
    assert_eq!(dto.status_bar_label, "Codex Session: 63% used · 37% left");
    assert_eq!(dto.buckets.len(), 1);
    assert_eq!(dto.buckets[0].remaining_percent, Some(37));
    assert_eq!(dto.buckets[0].resets_at, Some(99));
    assert_eq!(dto.status, "fresh");
    let merged = bridge.merged_status_bar_label().expect("merged");
    assert!(merged.contains("63%"));
    let surfaces = bridge.list_surfaces().expect("list");
    assert!(surfaces.iter().any(|s| s.id == "codex" && s.enabled));
    assert!(surfaces.iter().any(|s| s.id == "amp" && !s.enabled));
    bridge
        .set_enabled("codex".to_owned(), false)
        .expect("disable");
    assert!(bridge.snapshot("codex".to_owned()).is_err());
    bridge.shutdown().expect("shutdown");
    assert!(matches!(
        bridge.list_surfaces().expect_err("closed"),
        UsageBridgeError::RuntimeUnavailable | UsageBridgeError::Rejected { .. }
    ));
    // silence unused import warning if any
    let _ = HostUsageRuntime::new();
}

#[test]
fn bounded_events_and_refresh_floor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    assert_eq!(bridge.refresh_floor_secs().expect("floor"), 120);
    let batch = bridge.next_events(0, 50).expect("events");
    assert!(batch.next_cursor >= 1);
    assert!(batch.events.len() <= 256);
}
