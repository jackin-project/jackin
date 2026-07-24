// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::usage::{
    PercentStyle, ResetStyle, UsageFormatPrefs, estimate_caption, provider_display_label,
};
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

fn inject_remaining(runtime: &mut HostUsageRuntime, surface_id: &str, remaining: u8) {
    let mut view = FocusedUsageView::unavailable("seed", 1);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.status_bar_label = format!("{remaining}% left");
    view.buckets = vec![QuotaBucketView {
        label: "Session".to_owned(),
        used_label: Some(format!("{}% used", 100u8.saturating_sub(remaining))),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(remaining),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    runtime.inject_snapshot(surface_id, view).expect("inject");
}

/// Dual-bucket inject (session + weekly) for Desktop dual-line chip parity.
fn inject_dual_remaining(
    runtime: &mut HostUsageRuntime,
    surface_id: &str,
    session_remaining: u8,
    weekly_remaining: u8,
) {
    let mut view = FocusedUsageView::unavailable("seed", 1);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    view.status_bar_label = format!("{session_remaining}% left");
    view.buckets = vec![
        QuotaBucketView {
            label: "Session".to_owned(),
            used_label: Some(format!("{}% used", 100u8.saturating_sub(session_remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(session_remaining),
            reset_label: Some("Resets in 5h".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Session),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        },
        QuotaBucketView {
            label: "Weekly".to_owned(),
            used_label: Some(format!("{}% used", 100u8.saturating_sub(weekly_remaining))),
            limit_label: Some("100%".to_owned()),
            remaining_percent: Some(weekly_remaining),
            reset_label: Some("Resets in 2d".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Weekly),
            pace_label: Some("10% in reserve".to_owned()),
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        },
    ];
    runtime
        .inject_snapshot(surface_id, view)
        .expect("inject dual");
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
    // Claude precedes Codex in HostSurfaceId::ALL; default Left = remaining.
    assert_eq!(
        runtime.compact_status_bar_label().expect("compact"),
        "Cl 40%"
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
fn refresh_floor_skips_non_forced_calls() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig {
            data_dir: dir.path().to_path_buf(),
            refresh_floor_secs: 60,
            enabled_surface_ids: vec!["codex".to_owned()],
            probe_policy: HostProbePolicy::Live,
        })
        .expect("open");
    assert!(runtime.refresh_due());
    // force first refresh stamps last_refresh (may network or unavailable).
    runtime.refresh(Some("codex"), true).expect("force refresh");
    assert!(!runtime.refresh_due());
    // Non-forced call within floor must be a silent no-op.
    runtime.refresh(None, false).expect("floor skip");
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
fn compact_status_bar_strip_worst_first_cap_and_separator() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        let on = matches!(
            *surface,
            HostSurfaceId::Claude | HostSurfaceId::Codex | HostSurfaceId::Zai
        );
        runtime.set_enabled(surface.id(), on).expect("enable set");
    }
    inject_remaining(&mut runtime, "claude", 37); // 37% left — worst remaining
    inject_remaining(&mut runtime, "codex", 59); // 59% left
    inject_remaining(&mut runtime, "zai", 88); // 88% left
    // Worst-first by remaining: Claude, Codex, Z.AI.
    assert_eq!(
        runtime.compact_status_bar_strip(3).expect("strip"),
        "Cl 37% · Cx 59% · ZA 88%"
    );
    assert_eq!(runtime.compact_status_bar_strip(1).expect("cap1"), "Cl 37%");
}

/// Multi-provider strip: every enabled surface with numeric data contributes a token.
#[test]
fn compact_status_bar_strip_all_enabled_host_surfaces_with_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // Enable the full frozen catalog.
    for surface in HostSurfaceId::ALL {
        runtime.set_enabled(surface.id(), true).expect("enable set");
    }
    // Inject distinct remainings for five surfaces; leave others empty (hidden).
    inject_remaining(&mut runtime, "claude", 50);
    inject_remaining(&mut runtime, "codex", 40);
    inject_remaining(&mut runtime, "amp", 30);
    inject_remaining(&mut runtime, "grok", 20);
    inject_remaining(&mut runtime, "kimi", 10);
    let strip = runtime.compact_status_bar_strip(8).expect("strip");
    // Worst-first: lowest remaining first → Kimi, Grok, Amp, Codex, Claude.
    assert!(
        strip.contains("Ki ") && strip.contains("Gr ") && strip.contains("Am "),
        "strip should include per-provider compact tokens: {strip}"
    );
    assert!(
        strip.contains(" · "),
        "multi-provider strip joins with middle-dot separator: {strip}"
    );
    let parts: Vec<_> = strip.split(" · ").collect();
    assert!(
        parts.len() >= 5,
        "expected ≥5 provider tokens, got {}: {strip}",
        parts.len()
    );
    // Cap still applies.
    let capped = runtime.compact_status_bar_strip(2).expect("cap2");
    assert_eq!(capped.split(" · ").count(), 2, "cap2 strip: {capped}");
}

/// Dual-bucket surface still exposes both remainings via snapshot (Desktop chip stack).
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

#[test]
fn compact_depleted_with_and_without_resets_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        runtime
            .set_enabled(surface.id(), *surface == HostSurfaceId::Claude)
            .expect("enable set");
    }
    // Depleted without resets_at → remaining 0% (default Left).
    inject_remaining(&mut runtime, "claude", 0);
    assert_eq!(
        runtime
            .compact_status_bar_label()
            .expect("depleted no reset"),
        "Cl 0%"
    );
    runtime
        .set_format_prefs(UsageFormatPrefs {
            percent_style: PercentStyle::Used,
            reset_style: ResetStyle::Countdown,
        })
        .expect("prefs");
    assert_eq!(
        runtime
            .compact_status_bar_label()
            .expect("depleted used style"),
        "Cl 100%"
    );
    // Restore Left for the countdown branch below.
    runtime
        .set_format_prefs(UsageFormatPrefs::default())
        .expect("prefs left");

    // Depleted with resets_at in the future → "Cl resets …".
    let mut view = FocusedUsageView::unavailable("seed", 1);
    view.status = UsageSnapshotStatus::Fresh;
    view.source = UsageSource::ProviderApi;
    view.confidence = UsageConfidence::Authoritative;
    let future = chrono::Utc::now().timestamp() + 4_860; // 1h 21m
    view.buckets = vec![QuotaBucketView {
        label: "Session".to_owned(),
        used_label: Some("100% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(0),
        reset_label: None,
        resets_at: Some(future),
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Danger,
    }];
    runtime.inject_snapshot("claude", view).expect("inject");
    let label = runtime.compact_status_bar_label().expect("depleted");
    assert!(
        label.starts_with("Cl resets "),
        "expected depleted countdown form, got {label}"
    );
}

#[test]
fn next_refresh_label_due_and_countdown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    assert_eq!(runtime.next_refresh_label(), "Next update due");
    runtime.set_refresh_floor_secs(300).expect("floor");
    // Force a refresh mark without network by simulating last_refresh via
    // a non-forced path: inject is not a refresh. Use set_refresh after open:
    // calling refresh with force on empty targets still stamps last_refresh.
    runtime.refresh(None, true).expect("refresh stamp");
    let label = runtime.next_refresh_label();
    assert!(
        label.starts_with("Next update in ") || label == "Next update due",
        "got {label}"
    );
}

#[test]
fn overview_rows_numeric_and_status_word() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        let on = matches!(*surface, HostSurfaceId::Claude | HostSurfaceId::Codex);
        runtime.set_enabled(surface.id(), on).expect("enable set");
    }
    inject_remaining(&mut runtime, "claude", 97);
    let mut named = FocusedUsageView::unavailable("seed", 1);
    named.status = UsageSnapshotStatus::Fresh;
    named.source = UsageSource::ProviderApi;
    named.confidence = UsageConfidence::Authoritative;
    named.account.provider_label = "OpenAI / Codex".to_owned();
    named.buckets = vec![QuotaBucketView {
        label: "Fable".to_owned(),
        used_label: Some("32% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(68),
        reset_label: None,
        resets_at: Some(chrono::Utc::now().timestamp() + 86_400 * 2),
        status_slot: None,
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Warn,
    }];
    runtime.inject_snapshot("codex", named).expect("inject");

    let rows = runtime.overview_rows().expect("rows");
    assert_eq!(rows.len(), 2);
    let claude = rows.iter().find(|r| r.surface_id == "claude").expect("cl");
    assert_eq!(claude.headline, "97% left");
    assert_eq!(claude.status_word, "fresh");
    let codex = rows.iter().find(|r| r.surface_id == "codex").expect("cx");
    assert_eq!(codex.display_label, "OpenAI");
    assert_eq!(codex.headline, "Fable 68% left");
    assert_eq!(codex.severity, "warn");
    assert!(codex.reset_label.is_some());
    assert!(codex.exact_reset.is_some());

    // Prefs flip left → used on the same remaining data.
    runtime
        .set_format_prefs(UsageFormatPrefs {
            percent_style: PercentStyle::Used,
            reset_style: ResetStyle::ExactClock,
        })
        .expect("prefs");
    let rows = runtime.overview_rows().expect("rows2");
    let claude = rows.iter().find(|r| r.surface_id == "claude").expect("cl");
    assert_eq!(claude.headline, "3% used");
    let codex = rows.iter().find(|r| r.surface_id == "codex").expect("cx");
    let reset = codex.reset_label.as_deref().expect("reset");
    assert!(
        reset.starts_with("Resets ") && !reset.contains(" in "),
        "exact-clock form expected, got {reset}"
    );
}

/// OpenUsage/CodexBar-style: every frozen host surface can contribute a strip token.
#[test]
fn compact_status_bar_strip_all_eight_host_surfaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        runtime.set_enabled(surface.id(), true).expect("enable set");
    }
    // Distinct remainings so each surface has numeric data.
    let remainings = [90u8, 80, 70, 60, 50, 40, 30, 20];
    for (surface, rem) in HostSurfaceId::ALL.iter().zip(remainings.iter().copied()) {
        inject_remaining(&mut runtime, surface.id(), rem);
    }
    let strip = runtime.compact_status_bar_strip(8).expect("strip");
    // Remaining % default (Left): worst-first → lowest remaining first.
    assert!(
        strip.contains(" · "),
        "multi-provider strip separator: {strip}"
    );
    let parts: Vec<_> = strip.split(" · ").collect();
    assert_eq!(parts.len(), 8, "expected 8 provider tokens: {strip}");
    // Every compact prefix present.
    for surface in HostSurfaceId::ALL {
        let prefix = surface.compact_prefix();
        assert!(
            strip.contains(&format!("{prefix} ")),
            "missing {prefix} in strip: {strip}"
        );
    }
    // Worst-first: OpenCode 20% is first (lowest remaining).
    assert!(
        parts[0].starts_with("OC 20%"),
        "worst-first remaining, got {}",
        parts[0]
    );
}

#[test]
fn multi_account_list_select_and_snapshot() {
    use crate::host::{account_key_for_view, host_snapshot_store_path};
    use crate::usage_snapshot_store::store_usage_snapshot;

    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        runtime
            .set_enabled(surface.id(), *surface == HostSurfaceId::Claude)
            .expect("enable");
    }

    let mut account_a = FocusedUsageView::unavailable("seed", 1);
    account_a.status = UsageSnapshotStatus::Fresh;
    account_a.source = UsageSource::ProviderApi;
    account_a.confidence = UsageConfidence::Authoritative;
    account_a.account.provider_label = "Anthropic / Claude".to_owned();
    account_a.account.account_label = "personal@example.com".to_owned();
    account_a.account.plan_label = Some("Max".to_owned());
    account_a.status_bar_label = "50% left".to_owned();
    account_a.buckets = vec![QuotaBucketView {
        label: "Session".to_owned(),
        used_label: Some("50% used".to_owned()),
        limit_label: Some("100%".to_owned()),
        remaining_percent: Some(50),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Session),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }];
    let key_a = account_key_for_view(&account_a);
    let store = host_snapshot_store_path(dir.path());
    store_usage_snapshot(&store, &account_a).expect("store A");

    let mut account_b = account_a.clone();
    account_b.account.account_label = "work@company.com".to_owned();
    account_b.account.plan_label = Some("Team".to_owned());
    account_b.status_bar_label = "20% left".to_owned();
    account_b.buckets[0].remaining_percent = Some(20);
    account_b.buckets[0].used_label = Some("80% used".to_owned());
    let key_b = account_key_for_view(&account_b);
    runtime
        .inject_snapshot("claude", account_b)
        .expect("inject live B");

    let listed = runtime
        .list_accounts(Some("claude"))
        .expect("list accounts");
    assert!(
        listed.len() >= 2,
        "expected ≥2 accounts (store A + live B), got {}: {listed:?}",
        listed.len()
    );
    assert!(listed.iter().any(|a| a.account_key == key_a));
    assert!(listed.iter().any(|a| a.account_key == key_b));
    assert!(listed.iter().any(|a| a.account_label.contains("work@")));

    // Select durable personal account — snapshot must not invent, must return A.
    runtime
        .set_selected_account("claude", &key_a)
        .expect("select A");
    let snap = runtime.snapshot("claude").expect("snapshot A");
    assert_eq!(snap.account.account_label, "personal@example.com");
    assert_eq!(snap.buckets[0].remaining_percent, Some(50));

    runtime
        .set_selected_account("claude", &key_b)
        .expect("select B");
    let snap_b = runtime.snapshot("claude").expect("snapshot B");
    assert_eq!(snap_b.account.account_label, "work@company.com");
    assert_eq!(snap_b.buckets[0].remaining_percent, Some(20));
}

#[test]
fn provider_display_label_cases() {
    assert_eq!(provider_display_label("Codex"), "OpenAI");
    assert_eq!(provider_display_label("OpenAI / Codex"), "OpenAI");
    assert_eq!(provider_display_label("Claude"), "Anthropic");
    assert_eq!(provider_display_label("Anthropic / Claude"), "Anthropic");
    assert_eq!(provider_display_label("Grok Build"), "xAI");
    assert_eq!(provider_display_label("xAI / Grok"), "xAI");
    assert_eq!(provider_display_label("GLM / Z.AI"), "Z.AI");
    assert_eq!(provider_display_label("Amp"), "Amp");
}

#[test]
fn estimate_caption_variants() {
    let mut view = FocusedUsageView::unavailable("x", 1);
    view.confidence = UsageConfidence::Authoritative;
    view.source = UsageSource::ProviderApi;
    assert_eq!(estimate_caption(&view), None);

    view.confidence = UsageConfidence::Estimated;
    assert_eq!(
        estimate_caption(&view).as_deref(),
        Some("Estimated from token usage · not a subscription bill")
    );

    view.confidence = UsageConfidence::Authoritative;
    view.source = UsageSource::LocalLogs;
    assert_eq!(
        estimate_caption(&view).as_deref(),
        Some("Estimated from token usage · not a subscription bill")
    );

    view.source = UsageSource::Cli;
    view.confidence = UsageConfidence::PresenceOnly;
    assert_eq!(estimate_caption(&view), None);
}

// ===== Plan 005 Step 2: provider glance rows =====

fn glance_weekly_bucket(remaining: u8) -> QuotaBucketView {
    QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(remaining),
        reset_label: Some("Resets in 3d".to_owned()),
        resets_at: Some(1_700_200_000),
        status_slot: Some(StatusSlot::Weekly),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}

fn glance_daily_bucket(remaining: u8) -> QuotaBucketView {
    let mut bucket = glance_weekly_bucket(remaining);
    bucket.label = "Amp Free".to_owned();
    bucket.status_slot = Some(StatusSlot::Daily);
    bucket.reset_label = Some("Resets daily".to_owned());
    bucket.resets_at = None;
    bucket
}

fn glance_view(
    provider_label: &str,
    origin: Option<&str>,
    buckets: Vec<QuotaBucketView>,
    status: UsageSnapshotStatus,
) -> FocusedUsageView {
    FocusedUsageView {
        focused_agent: None,
        focused_provider: Some(provider_label.to_owned()),
        account: FocusedAccountHeader {
            provider_label: provider_label.to_owned(),
            account_label: "user@example.com".to_owned(),
            username: None,
            plan_label: None,
            credential_origin: origin.map(str::to_owned),
        },
        buckets,
        status,
        source: UsageSource::ProviderApi,
        confidence: UsageConfidence::Authoritative,
        fetched_at_epoch: 1_699_000_000,
        updated_label: "just now".to_owned(),
        status_bar_label: String::new(),
        tabs: Vec::new(),
        last_error: None,
    }
}

#[test]
fn provider_glance_rows_use_exact_seven_provider_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for id in ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"] {
        runtime
            .inject_snapshot(
                id,
                glance_view(
                    "P",
                    Some("OAuth · file"),
                    vec![glance_weekly_bucket(50)],
                    UsageSnapshotStatus::Fresh,
                ),
            )
            .expect("inject");
    }
    let rows = runtime.provider_glance_rows().expect("rows");
    let ids: Vec<_> = rows.iter().map(|r| r.surface_id.as_str()).collect();
    assert_eq!(
        ids,
        ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"]
    );
}

#[test]
fn provider_glance_rows_show_three_weekly_labels() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(57)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    runtime
        .inject_snapshot(
            "claude",
            glance_view(
                "Claude",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(74)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    runtime
        .inject_snapshot(
            "zai",
            glance_view(
                "GLM / Z.AI",
                Some("API key · env ZAI_API_KEY"),
                vec![glance_weekly_bucket(31)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let bar = |id: &str| {
        rows.iter()
            .find(|r| r.surface_id == id)
            .map(|r| r.bar_label.clone())
    };
    assert_eq!(bar("codex").as_deref(), Some("57%"));
    assert_eq!(bar("claude").as_deref(), Some("74%"));
    assert_eq!(bar("zai").as_deref(), Some("31%"));
}

#[test]
fn provider_glance_rows_select_amp_daily() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "amp",
            glance_view(
                "Amp",
                Some("API key · env AMP_API_KEY"),
                vec![glance_daily_bucket(61)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let amp = rows
        .iter()
        .find(|r| r.surface_id == "amp")
        .expect("amp row");
    assert_eq!(amp.bar_label, "61%");
    assert_eq!(amp.glance_remaining_percent, Some(61));
}

#[test]
fn provider_glance_rows_show_dash_for_paid_only_amp() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // Credit-bound bucket but no Daily slot → detected but no glance percent.
    let mut credits = glance_weekly_bucket(40);
    credits.label = "Individual credits".to_owned();
    credits.status_slot = None;
    credits.remaining_percent = None;
    credits.limit_label = Some("$9.86".to_owned());
    runtime
        .inject_snapshot(
            "amp",
            glance_view(
                "Amp",
                Some("API key · env AMP_API_KEY"),
                vec![credits],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let amp = rows
        .iter()
        .find(|r| r.surface_id == "amp")
        .expect("amp row");
    assert_eq!(amp.bar_label, "–");
    assert_eq!(amp.glance_remaining_percent, None);
}

#[test]
fn provider_glance_rows_show_dash_before_first_success() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                Vec::new(),
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let codex = rows
        .iter()
        .find(|r| r.surface_id == "codex")
        .expect("codex row");
    assert_eq!(codex.bar_label, "–");
    assert_eq!(codex.headline, "–");
}

#[test]
fn provider_glance_rows_empty_without_credentials() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let rows = runtime.provider_glance_rows().expect("rows");
    assert!(rows.is_empty());
}

#[test]
fn provider_glance_rows_reject_negative_credential_placeholders() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "zai",
            glance_view(
                "GLM / Z.AI",
                Some("needs env ZAI_API_KEY"),
                Vec::new(),
                UsageSnapshotStatus::NeedsLogin,
            ),
        )
        .expect("inject");
    runtime
        .inject_snapshot(
            "kimi",
            glance_view(
                "Kimi",
                Some("needs Kimi auth"),
                Vec::new(),
                UsageSnapshotStatus::NeedsLogin,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    assert!(rows.is_empty());
}

#[test]
fn provider_glance_rows_accept_affirmative_origin_when_unsupported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "kimi",
            glance_view(
                "Kimi",
                Some("API key · env KIMI_AUTH_TOKEN"),
                Vec::new(),
                UsageSnapshotStatus::Unsupported,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let kimi = rows
        .iter()
        .find(|r| r.surface_id == "kimi")
        .expect("kimi detected");
    assert_eq!(kimi.bar_label, "–");
}

#[test]
fn provider_glance_rows_do_not_fallback_to_unrelated_slots() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let mut session = glance_weekly_bucket(80);
    session.status_slot = Some(StatusSlot::Session);
    let mut spend = glance_weekly_bucket(20);
    spend.status_slot = Some(StatusSlot::Spend);
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                vec![session, spend],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let codex = rows
        .iter()
        .find(|r| r.surface_id == "codex")
        .expect("codex row");
    assert_eq!(codex.bar_label, "–");
    assert_eq!(codex.glance_remaining_percent, None);
}

#[test]
fn provider_glance_rows_never_include_opencode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "opencode",
            glance_view(
                "OpenCode",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(90)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    assert!(rows.iter().all(|r| r.surface_id != "opencode"));
}

#[test]
fn provider_glance_rows_icon_keys_match_closed_desktop_domain() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    runtime
        .inject_snapshot(
            "grok",
            glance_view(
                "Grok Build",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(33)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let grok = rows
        .iter()
        .find(|r| r.surface_id == "grok")
        .expect("grok row");
    assert_eq!(grok.icon_key, "grok");
    assert_eq!(grok.icon_key, grok.surface_id);
}

#[test]
fn provider_glance_rows_preserve_dimmed_last_known() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    let mut stale = glance_view(
        "Codex",
        Some("OAuth · file"),
        vec![glance_weekly_bucket(45)],
        UsageSnapshotStatus::Stale,
    );
    stale.buckets[0].status = UsageSnapshotStatus::Stale;
    runtime.inject_snapshot("codex", stale).expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let codex = rows
        .iter()
        .find(|r| r.surface_id == "codex")
        .expect("codex row");
    assert_eq!(codex.bar_label, "45%");
    assert!(codex.dimmed);
}

#[test]
fn provider_glance_rows_marks_canonical_placeholder_refreshing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    // A first-ever placeholder without prior evidence is absent.
    runtime
        .inject_snapshot("codex", FocusedUsageView::refreshing(Some("Codex"), 0))
        .expect("inject");
    assert!(
        runtime
            .provider_glance_rows()
            .expect("rows")
            .iter()
            .all(|r| r.surface_id != "codex")
    );
    // Establish evidence, then replace with the placeholder → retained + refreshing.
    runtime
        .inject_snapshot(
            "codex",
            glance_view(
                "Codex",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(50)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    assert!(
        runtime
            .provider_glance_rows()
            .expect("rows")
            .iter()
            .any(|r| r.surface_id == "codex")
    );
    runtime
        .inject_snapshot("codex", FocusedUsageView::refreshing(Some("Codex"), 0))
        .expect("inject");
    let rows = runtime.provider_glance_rows().expect("rows");
    let codex = rows
        .iter()
        .find(|r| r.surface_id == "codex")
        .expect("retained");
    assert!(codex.is_refreshing);
}

#[test]
fn provider_glance_rows_redetect_new_credentials() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    assert!(runtime.provider_glance_rows().expect("rows").is_empty());
    runtime
        .inject_snapshot(
            "grok",
            glance_view(
                "Grok Build",
                Some("OAuth · file"),
                vec![glance_weekly_bucket(33)],
                UsageSnapshotStatus::Fresh,
            ),
        )
        .expect("inject");
    assert!(
        runtime
            .provider_glance_rows()
            .expect("rows")
            .iter()
            .any(|r| r.surface_id == "grok")
    );
}

#[test]
fn disabled_probe_policy_skips_dispatch_and_is_never_due() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = HostUsageRuntime::new();
    runtime
        .open(HostRuntimeConfig {
            data_dir: dir.path().to_path_buf(),
            refresh_floor_secs: 60,
            enabled_surface_ids: Vec::new(),
            probe_policy: HostProbePolicy::Disabled,
        })
        .expect("open");
    runtime.refresh(None, false).expect("non-forced refresh");
    runtime.refresh(None, true).expect("forced refresh");
    assert!(!runtime.refresh_due());
}
