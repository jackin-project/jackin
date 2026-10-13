// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

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
fn compact_status_bar_label_picks_lowest_remaining_percent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // Only claude + codex enabled.
    for surface in HostSurfaceId::ALL {
        let on = matches!(*surface, HostSurfaceId::Claude | HostSurfaceId::Codex);
        runtime.set_enabled(surface.id(), on).expect("enable set");
    }
    inject_remaining(&mut runtime, "claude", 50); // 50% left
    inject_remaining(&mut runtime, "codex", 18); // 18% left — worst
    assert_eq!(
        runtime.compact_status_bar_label().expect("compact"),
        "Cx 18%"
    );

    // PercentStyle::Used flips the same driving remaining to used %.
    runtime
        .set_format_prefs(UsageFormatPrefs {
            percent_style: PercentStyle::Used,
            reset_style: ResetStyle::Countdown,
        })
        .expect("prefs");
    assert_eq!(
        runtime.compact_status_bar_label().expect("compact used"),
        "Cx 82%"
    );
}

#[test]
fn compact_status_bar_label_tie_keeps_all_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        let on = matches!(*surface, HostSurfaceId::Claude | HostSurfaceId::Codex);
        runtime.set_enabled(surface.id(), on).expect("enable set");
    }
    inject_remaining(&mut runtime, "claude", 40);
    inject_remaining(&mut runtime, "codex", 40);
    // OpenAI precedes Anthropic in the settled host provider order.
    assert_eq!(
        runtime.compact_status_bar_label().expect("compact"),
        "Cx 40%"
    );
}

#[test]
fn compact_status_bar_label_empty_when_unavailable_or_disabled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // All enabled but no numeric remaining (unavailable inject has empty buckets).
    let unavailable = FocusedUsageView::unavailable("missing", 1);
    runtime
        .inject_snapshot("claude", unavailable)
        .expect("inject");
    assert_eq!(
        runtime.compact_status_bar_label().expect("compact"),
        "",
        "unavailable without remaining_percent must not invent %"
    );

    inject_remaining(&mut runtime, "codex", 10);
    for surface in HostSurfaceId::ALL {
        runtime.set_enabled(surface.id(), false).expect("disable");
    }
    assert_eq!(
        runtime.compact_status_bar_label().expect("compact"),
        "",
        "all-disabled must yield empty compact label"
    );
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
    assert_eq!(
        bucket.used_money.as_ref().map(|m| m.currency.as_str()),
        Some("SGD")
    );
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
fn refresh_floor_tracks_completed_broker_refresh() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig {
            data_dir: dir.path().to_path_buf(),
            refresh_floor_secs: 60,
            enabled_surface_ids: vec!["codex".to_owned()],
            probe_policy: HostProbePolicy::Live,
            discovery_scope: UsageDiscoveryScope::Capsule {
                forwarded_accounts: Vec::new(),
            },
        })
        .expect("open");
    assert!(runtime.refresh_due());
    runtime.last_refresh = Some(Instant::now());
    assert!(!runtime.refresh_due());
    // Floor mutator clamps and is readable.
    runtime.set_refresh_floor_secs(30).expect("set floor");
    assert_eq!(runtime.refresh_floor_secs(), 60);
    runtime.set_refresh_floor_secs(120).expect("set floor");
    assert_eq!(runtime.refresh_floor_secs(), 120);
}

#[test]
fn next_events_resync_flag_not_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // Cursor far behind empty-ish log after open: if we drop events by flooding
    // past MAX_EVENT_LOG, resync becomes true.
    for _ in 0..5_000 {
        runtime.set_enabled("amp", false).expect("toggle");
        runtime.set_enabled("amp", true).expect("toggle");
    }
    let batch = runtime.next_events(0, 10).expect("events");
    // Either resync (cursor 0 behind first retained) or events — never Err.
    if batch.resync_required {
        assert!(batch.events.is_empty());
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

#[test]
fn compact_status_bar_label_for_pinned_known_and_disabled() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    inject_remaining(&mut runtime, "claude", 37); // 37% left (default Left)
    assert_eq!(
        runtime
            .compact_status_bar_label_for("claude")
            .expect("pinned"),
        Some("Cl 37%".to_owned())
    );
    runtime.set_enabled("claude", false).expect("disable");
    assert_eq!(
        runtime
            .compact_status_bar_label_for("claude")
            .expect("disabled"),
        None
    );
    assert_eq!(
        runtime
            .compact_status_bar_label_for("codex")
            .expect("no data"),
        None
    );
}

#[test]
fn compact_status_bar_strip_soonest_then_remaining_cap_and_separator() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        let on = matches!(
            *surface,
            HostSurfaceId::Claude | HostSurfaceId::Codex | HostSurfaceId::Zai
        );
        runtime.set_enabled(surface.id(), on).expect("enable set");
    }
    // No resets_at → SB-17 time key ties; higher remaining ranks first.
    inject_remaining(&mut runtime, "claude", 37);
    inject_remaining(&mut runtime, "codex", 59);
    inject_remaining(&mut runtime, "zai", 88);
    assert_eq!(
        runtime.compact_status_bar_strip(3).expect("strip"),
        "ZA 88% · Cx 59% · Cl 37%"
    );
    assert_eq!(runtime.compact_status_bar_strip(1).expect("cap1"), "ZA 88%");
}

#[test]
fn compact_status_bar_strip_hard_cap_three_and_hides_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        runtime.set_enabled(surface.id(), true).expect("enable set");
    }
    inject_remaining(&mut runtime, "claude", 50);
    inject_remaining(&mut runtime, "codex", 40);
    inject_remaining(&mut runtime, "amp", 30);
    inject_remaining(&mut runtime, "grok", 20);
    inject_remaining(&mut runtime, "kimi", 10);
    inject_remaining(&mut runtime, "zai", 0); // SB-19: out
    // max=8 still hard-capped to 3 (SB-3).
    let strip = runtime.compact_status_bar_strip(8).expect("strip");
    let parts: Vec<_> = strip.split(" · ").collect();
    assert_eq!(parts.len(), 3, "SB-3 hard cap 3, got {strip}");
    assert!(
        !strip.contains("ZA "),
        "0% Z.AI must not appear on burn-first bar: {strip}"
    );
    // No reset epochs → higher remaining first among the five non-zero.
    assert_eq!(
        parts[0], "Cl 50%",
        "highest remaining first when times tie: {strip}"
    );
    let capped = runtime.compact_status_bar_strip(2).expect("cap2");
    assert_eq!(capped.split(" · ").count(), 2, "cap2 strip: {capped}");
}

#[test]
fn compact_status_bar_strip_soonest_reset_beats_higher_remaining() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        let on = matches!(*surface, HostSurfaceId::Claude | HostSurfaceId::Codex);
        runtime.set_enabled(surface.id(), on).expect("enable set");
    }
    let soon = 1_700_000_000_i64;
    let later = soon + 86_400;
    inject_remaining_at(&mut runtime, "claude", 90, Some(later)); // high rem, later reset
    inject_remaining_at(&mut runtime, "codex", 40, Some(soon)); // lower rem, sooner reset
    let strip = runtime.compact_status_bar_strip(3).expect("strip");
    assert!(
        strip.starts_with("Cx "),
        "soonest reset (Codex) ranks first: {strip}"
    );
    assert!(strip.contains("Cl "), "Claude still second: {strip}");
}

#[test]
fn status_bar_provider_glance_rows_sb3_sb17_sb19() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::DESKTOP_PROVIDER_ORDER {
        runtime.set_enabled(surface.id(), true).expect("enable set");
    }
    let soon = 1_700_000_100_i64;
    inject_remaining_at(&mut runtime, "claude", 12, Some(soon + 10_000));
    inject_remaining_at(&mut runtime, "codex", 57, Some(soon)); // soonest
    inject_remaining_at(&mut runtime, "amp", 100, Some(soon + 20_000));
    inject_remaining_at(&mut runtime, "grok", 72, Some(soon + 5_000));
    inject_remaining(&mut runtime, "kimi", 0); // hidden
    // Full inventory still includes 0% Kimi for popover.
    let inventory = runtime.provider_glance_rows().expect("inventory");
    assert!(
        inventory.iter().any(|r| r.surface_id == "kimi"),
        "popover inventory keeps 0% rows"
    );
    let bar = runtime.status_bar_provider_glance_rows(8).expect("bar");
    assert_eq!(bar.len(), 3, "SB-3 hard cap");
    assert_eq!(bar[0].surface_id, "codex", "soonest reset first");
    assert!(
        bar.iter().all(|r| r.surface_id != "kimi"),
        "SB-19: no 0% on bar"
    );
    assert!(
        bar.iter().all(|r| r.glance_remaining_percent != Some(0)),
        "no zero remaining on bar"
    );
}

#[test]
fn dual_bucket_snapshot_exposes_session_and_weekly_remainings() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        runtime
            .set_enabled(surface.id(), *surface == HostSurfaceId::Claude)
            .expect("enable set");
    }
    inject_dual_remaining(&mut runtime, "claude", 100, 79);
    let snap = runtime.snapshot("claude").expect("snapshot");
    let remainings: Vec<u8> = snap
        .buckets
        .iter()
        .filter_map(|b| b.remaining_percent)
        .collect();
    assert_eq!(
        remainings,
        vec![100, 79],
        "session then weekly remainings for dual-line chips"
    );
    assert_eq!(snap.buckets[0].label, "Session");
    assert_eq!(snap.buckets[1].label, "Weekly");
    assert!(
        snap.buckets[1].pace_label.as_deref() == Some("10% in reserve"),
        "pace present for Desktop two-column caption"
    );
}
