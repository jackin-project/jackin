// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn overview_rows_and_format_prefs_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    {
        let mut guard = bridge.inner.lock().expect("lock");
        let view = FocusedUsageView {
            focused_agent: Some("claude".to_owned()),
            focused_provider: Some("Claude".to_owned()),
            account: FocusedAccountHeader {
                provider_label: "Anthropic / Claude".to_owned(),
                account_label: "a@b.c".to_owned(),
                username: None,
                plan_label: None,
                credential_origin: None,
            },
            buckets: vec![QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("3% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(97),
                reset_label: None,
                resets_at: Some(1_900_000_000),
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
            status_bar_label: "ok".to_owned(),
            tabs: Vec::new(),
            last_error: None,
        };
        guard.inject_snapshot("claude", view).expect("inject");
    }
    {
        let mut guard = bridge.inner.lock().expect("lock");
        let view = FocusedUsageView {
            focused_agent: Some("codex".to_owned()),
            focused_provider: Some("Codex".to_owned()),
            account: FocusedAccountHeader {
                provider_label: "OpenAI / Codex".to_owned(),
                account_label: "c@d.e".to_owned(),
                username: None,
                plan_label: None,
                credential_origin: None,
            },
            buckets: vec![QuotaBucketView {
                label: "Session".to_owned(),
                used_label: Some("41% used".to_owned()),
                limit_label: Some("100%".to_owned()),
                remaining_percent: Some(59),
                reset_label: None,
                resets_at: None,
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
            status_bar_label: "ok".to_owned(),
            tabs: Vec::new(),
            last_error: None,
        };
        guard.inject_snapshot("codex", view).expect("inject");
    }

    let rows = bridge.overview_rows().expect("rows");
    assert!(
        rows.iter()
            .any(|r| r.surface_id == "claude" && r.headline == "97% left")
    );
    assert!(rows.iter().any(|r| r.display_label == "Anthropic"));

    bridge
        .set_format_prefs(UsageFormatPrefsDto {
            percent_style: "used".to_owned(),
            reset_style: "exact_clock".to_owned(),
        })
        .expect("prefs");
    let rows = bridge.overview_rows().expect("rows2");
    let claude = rows.iter().find(|r| r.surface_id == "claude").expect("cl");
    assert_eq!(claude.headline, "3% used");

    assert_eq!(
        bridge
            .compact_status_bar_label_for("claude".to_owned())
            .expect("pinned")
            .as_deref(),
        Some("Cl 3%")
    );
    let strip = bridge.compact_status_bar_strip(2).expect("strip");
    assert!(
        strip.contains(" · ") || strip.starts_with("Cl ") || strip.starts_with("Cx "),
        "strip={strip}"
    );
    let next = bridge.next_refresh_label().expect("next");
    assert!(
        next == "Next update due" || next.starts_with("Next update in "),
        "next={next}"
    );
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

#[test]
fn provider_glance_rows_via_bridge_project_rust_rows() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    {
        let mut guard = bridge.inner.lock().expect("lock");
        let mut view = FocusedUsageView::unavailable("seed", 1);
        view.status = UsageSnapshotStatus::Fresh;
        view.source = UsageSource::ProviderApi;
        view.confidence = UsageConfidence::Authoritative;
        view.account.provider_label = "OpenAI / Codex".to_owned();
        view.account.account_label = "codex@example.com".to_owned();
        view.account.credential_origin = Some("OAuth · ~/.codex/auth.json".to_owned());
        view.last_error = None;
        view.buckets = vec![QuotaBucketView {
            label: "Weekly".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(57),
            reset_label: Some("Resets in 3d".to_owned()),
            resets_at: Some(1_700_200_000),
            status_slot: Some(StatusSlot::Weekly),
            pace_label: None,
            status: UsageSnapshotStatus::Fresh,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        }];
        guard.inject_snapshot("codex", view).expect("inject");
    }
    let rows = bridge.provider_glance_rows().expect("glance rows");
    let codex = rows
        .iter()
        .find(|row| row.surface_id == "codex")
        .expect("codex glance row");
    assert_eq!(codex.icon_key, "codex");
    assert_eq!(codex.fallback_glyph, "Cx");
    assert!(
        codex
            .usage_url
            .as_deref()
            .is_some_and(|url| url.contains("usage"))
    );
    assert_eq!(codex.bar_label, "57%");
    assert_eq!(codex.glance_remaining_percent, Some(57));
    assert!(!codex.is_refreshing);

    let inventory = bridge.desktop_inventory().expect("desktop inventory");
    assert_eq!(inventory.groups.len(), 1);
    let codex = &inventory.groups[0];
    assert_eq!(codex.surface_id, "codex");
    assert_eq!(codex.display_label, "OpenAI");
    assert_eq!(codex.accounts.len(), 1);
    let account = &codex.accounts[0];
    assert_eq!(account.account_label, "codex@example.com");
    assert!(account.selected);
    assert_eq!(account.lifecycle, "current");
    assert_eq!(account.provenance, ["Live host"]);
    assert_eq!(account.remaining_percent, Some(57));
    assert_eq!(account.remaining_label, "57%");
    assert_eq!(account.headline, "57% left");
    assert_eq!(account.reset_display_label, "Resets now");
    assert_eq!(account.status_word, "fresh");
    assert_eq!(account.plan_or_status_label, "—");
    assert_eq!(account.last_error, None);

    let projection = bridge.desktop_projection(3).expect("desktop projection");
    assert_eq!(projection.providers.len(), 1);
    assert_eq!(projection.glance_rows.len(), 1);
    assert_eq!(projection.status_bar_glance_rows.len(), 1);
    let provider = &projection.providers[0];
    assert_eq!(provider.group.surface_id, "codex");
    assert_eq!(provider.selected_account_route.status, "available");
    assert_eq!(
        provider.selected_account_route.account_key.as_deref(),
        Some(account.account_key.as_str())
    );
    assert_eq!(provider.selected_account_route.notice, None);
    assert_eq!(provider.selected_usage.identity.provider_title, "OpenAI");
    assert_eq!(
        provider.selected_usage.identity.account_label,
        "codex@example.com"
    );
    assert_eq!(provider.selected_usage.identity.activity_kind, "idle");
    assert!(
        provider
            .selected_usage
            .detail_presentation
            .rows
            .iter()
            .any(|row| row.row_id == "provenance")
    );
}

#[test]
fn detail_presentation_rides_the_snapshot_dto() {
    let dir = tempfile::tempdir().expect("tempdir");
    let bridge = open_bridge(dir.path());
    {
        let mut guard = bridge.inner.lock().expect("lock");
        let mut view = FocusedUsageView::unavailable("seed", 1);
        view.status = UsageSnapshotStatus::Stale;
        view.source = UsageSource::ProviderApi;
        view.confidence = UsageConfidence::Authoritative;
        view.focused_agent = Some("codex".to_owned());
        view.focused_provider = Some("OpenAI".to_owned());
        view.account.provider_label = "OpenAI".to_owned();
        view.account.account_label = "codex@example.com".to_owned();
        view.updated_label = "Updated 2m ago".to_owned();
        view.last_error = Some("upstream 503".to_owned());
        let weekly = |rem| QuotaBucketView {
            label: "Weekly".to_owned(),
            used_label: None,
            limit_label: None,
            remaining_percent: Some(rem),
            reset_label: Some("Resets in 3d".to_owned()),
            resets_at: None,
            status_slot: Some(StatusSlot::Weekly),
            pace_label: None,
            status: UsageSnapshotStatus::Stale,
            used_money: None,
            limit_money: None,
            severity: UsageSeverity::Normal,
        };
        view.buckets = vec![weekly(80), weekly(20)];
        guard.inject_snapshot("codex", view).expect("inject");
    }
    let dto = bridge.snapshot("codex".to_owned()).expect("snapshot");
    let rows = &dto.detail_presentation.rows;
    let ids: Vec<&str> = rows.iter().map(|r| r.row_id.as_str()).collect();
    assert_eq!(ids, vec!["bucket:0", "bucket:1", "detail"]);
    assert_eq!(dto.identity.provider_title, "OpenAI");
    assert_eq!(dto.identity.account_label, "codex@example.com");
    assert!(
        dto.identity
            .activity_label
            .starts_with("Update delayed · Updated "),
        "{}",
        dto.identity.activity_label
    );
    assert_eq!(dto.identity.activity_kind, "exceptional");
    // Duplicate labels keep distinct ids and distinct values.
    let b0 = rows.iter().find(|r| r.row_id == "bucket:0").expect("b0");
    let b1 = rows.iter().find(|r| r.row_id == "bucket:1").expect("b1");
    assert_eq!(b0.label, "Weekly");
    assert_eq!(b1.label, "Weekly");
    assert!(b0.display_label.starts_with("80% left"));
    assert!(b1.display_label.starts_with("20% left"));
    assert_eq!(b0.kind, "bucket");
    // Reset segment is the trailing column; line grouping survives FFI.
    let reset_line = b0
        .layout_lines
        .iter()
        .find(|line| line.trailing.is_some())
        .expect("reset line");
    assert_eq!(reset_line.leading, None);
    assert_eq!(reset_line.trailing.as_deref(), Some("Resets in 3d"));
    // Exactly one Detail row, appended after the last-good buckets.
    let detail: Vec<&crate::dto::UsageDetailRowDto> =
        rows.iter().filter(|r| r.kind == "detail").collect();
    assert_eq!(detail.len(), 1);
    assert_eq!(detail[0].display_label, "upstream 503");
}
