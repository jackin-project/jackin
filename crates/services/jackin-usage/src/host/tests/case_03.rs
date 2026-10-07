// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::usage_snapshot_store::store_usage_snapshots;

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
    runtime.last_refresh = Some(Instant::now());
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

#[test]
fn compact_status_bar_strip_all_eight_host_surfaces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut runtime = open_runtime(dir.path());
    for surface in HostSurfaceId::ALL {
        runtime.set_enabled(surface.id(), true).expect("enable set");
    }
    // Distinct remainings so each surface has numeric data (no resets → higher rem first).
    let remainings = [90u8, 80, 70, 60, 50, 40, 30, 20];
    for (surface, rem) in HostSurfaceId::ALL.iter().zip(remainings.iter().copied()) {
        inject_remaining(&mut runtime, surface.id(), rem);
    }
    let strip = runtime.compact_status_bar_strip(8).expect("strip");
    assert!(
        strip.contains(" · "),
        "multi-provider strip separator: {strip}"
    );
    let parts: Vec<_> = strip.split(" · ").collect();
    assert_eq!(
        parts.len(),
        3,
        "SB-3 hard-caps burn-first strip at 3: {strip}"
    );
    // No reset epochs → highest remaining among ALL ranks first (OpenAI 90%).
    assert!(
        parts[0].starts_with("Cx 90%"),
        "SB-17 higher-remaining first when times tie, got {}",
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

    let account_a = seeded_personal_account();
    let key_a = account_key_for_view(&account_a).expect("canonical key A");
    let store = host_snapshot_store_path(dir.path());
    store_usage_snapshot(&store, &account_a).expect("store A");

    let mut account_b = account_a.clone();
    account_b.account.account_label = "work@company.com".to_owned();
    account_b.account.plan_label = Some("Team".to_owned());
    account_b.status_bar_label = "20% left".to_owned();
    account_b.buckets[0].remaining_percent = Some(20);
    account_b.buckets[0].used_label = Some("80% used".to_owned());
    let key_b = account_key_for_view(&account_b).expect("canonical key B");
    runtime
        .inject_snapshot("claude", account_b)
        .expect("inject live B");

    let listed = runtime
        .list_accounts(Some("claude"))
        .expect("list accounts");
    assert_eq!(listed.len(), 2, "store A + live B: {listed:?}");
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
    let selected_projection = runtime
        .desktop_projection(3)
        .expect("available account projection");
    let claude = selected_projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "claude")
        .expect("Claude projection");
    assert_eq!(
        claude.selected_account_route,
        HostSelectedAccountRoute::Available {
            account_key: key_a.clone()
        }
    );

    // The current catalog removes A while sibling B remains. The persisted
    // selection is intent, so it stays explicit and unavailable instead of
    // being rewritten to B.
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: Some("only-b-generation".to_owned()),
        accounts: vec![canonical_discovered_account(
            HostSurfaceId::Claude,
            "work@company.com",
        )],
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: Vec::new(),
    });
    let unavailable = runtime.snapshot("claude").expect("unavailable A");
    assert_eq!(unavailable.status, UsageSnapshotStatus::Unavailable);
    assert_eq!(
        unavailable.last_error.as_deref(),
        Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
    );
    assert_ne!(unavailable.account.account_label, "work@company.com");
    let persisted = accounts::load_selected_accounts(&selected_accounts_path(dir.path()));
    assert_eq!(persisted.get("claude"), Some(&key_a));

    let glance = runtime
        .provider_glance_rows()
        .expect("unavailable glance row");
    let claude = glance
        .iter()
        .find(|row| row.surface_id == "claude")
        .expect("Claude glance row");
    assert_eq!(
        claude.last_error.as_deref(),
        Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
    );
    assert_eq!(claude.glance_remaining_percent, None);

    let projection = runtime
        .desktop_projection(3)
        .expect("unavailable desktop projection");
    let claude = projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "claude")
        .expect("Claude desktop projection");
    assert_eq!(
        claude.selected_account_route,
        HostSelectedAccountRoute::Unavailable {
            account_key: key_a.clone(),
            notice: SELECTED_ACCOUNT_UNAVAILABLE_NOTICE,
        }
    );
    assert_eq!(
        claude.selected_usage.last_error.as_deref(),
        Some(SELECTED_ACCOUNT_UNAVAILABLE_NOTICE)
    );
    assert!(
        claude
            .group
            .accounts
            .iter()
            .all(|account| !account.selected)
    );

    // Reappearing canonical A restores the valid selection without churn.
    runtime.discovery = Some(ValidatedUsageDiscovery {
        config_generation: Some("a-and-b-generation".to_owned()),
        accounts: vec![
            canonical_discovered_account(HostSurfaceId::Claude, "personal@example.com"),
            canonical_discovered_account(HostSurfaceId::Claude, "work@company.com"),
        ],
        diagnostics: Vec::new(),
        candidates: Vec::new(),
        bindings: Vec::new(),
    });
    let restored = runtime.snapshot("claude").expect("restored A");
    assert_eq!(restored.account.account_label, "personal@example.com");
    assert_eq!(restored.buckets[0].remaining_percent, Some(50));
    let restored_projection = runtime
        .desktop_projection(3)
        .expect("restored account projection");
    let claude = restored_projection
        .providers
        .iter()
        .find(|provider| provider.group.surface_id == "claude")
        .expect("Claude restored projection");
    assert_eq!(
        claude.selected_account_route,
        HostSelectedAccountRoute::Available {
            account_key: key_a.clone()
        }
    );

    runtime
        .set_selected_account("claude", &key_b)
        .expect("select B");
    let snap_b = runtime.snapshot("claude").expect("snapshot B");
    assert_eq!(snap_b.account.account_label, "work@company.com");
    assert_eq!(snap_b.buckets[0].remaining_percent, Some(20));
}

#[test]
fn materialize_account_catalog_reads_durable_history_through_snapshot_seam() {
    let dir = tempfile::tempdir().expect("tempdir");
    let view = codex_fixture_view();
    let store_path = host_snapshot_store_path(dir.path());
    store_usage_snapshots(&store_path, std::slice::from_ref(&view)).expect("seed store");

    let mut runtime = open_runtime(dir.path());
    let catalog = runtime
        .materialize_account_catalog()
        .expect("account catalog");

    let entries = catalog.entries_for_surface(HostSurfaceId::Codex);
    let historical: Vec<_> = entries
        .iter()
        .filter(|entry| entry.lifecycle == AccountLifecycle::Historical)
        .collect();
    assert_eq!(historical.len(), 1, "durable rows must materialize once");
    assert_eq!(historical[0].account_label, "codex@example.com");
    assert!(!historical[0].view.buckets.is_empty());
}
