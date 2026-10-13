// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_detail_presentation_preserves_exact_capsule_row_order() {
    let session = presentation_bucket(
        "Session",
        Some(97),
        Some(StatusSlot::Session),
        UsageSnapshotStatus::Fresh,
    );
    let view = detail_view(vec![session], None, UsageSnapshotStatus::Fresh);
    let presentation = usage_detail_presentation(&view);
    let ids: Vec<&str> = presentation
        .rows
        .iter()
        .map(|r| r.row_id.as_str())
        .collect();
    assert_eq!(ids, vec!["username", "plan", "auth", "bucket:0"]);
    assert!(!ids.iter().any(|id| matches!(
        *id,
        "focused" | "header" | "provider" | "account" | "status" | "updated"
    )));
    assert_eq!(presentation.rows[0].display_label, "operator");
}

#[test]
fn usage_identity_presentation_owns_account_and_activity_copy() {
    let view = detail_view(Vec::new(), None, UsageSnapshotStatus::Fresh);
    let idle = usage_identity_presentation("OpenAI", &view, false);
    assert_eq!(idle.provider_title, "OpenAI");
    assert_eq!(idle.account_label, "operator@example.com");
    assert_eq!(idle.activity_label, "Updated 2m ago");
    assert_eq!(
        idle.activity_kind,
        jackin_protocol::control::UsageActivityKind::Idle
    );
    assert_eq!(
        idle.accessibility_label,
        "OpenAI, operator@example.com, Updated 2m ago"
    );

    let updating = usage_identity_presentation("OpenAI", &view, true);
    assert_eq!(updating.activity_label, "Updating…");
    assert_eq!(
        updating.activity_kind,
        jackin_protocol::control::UsageActivityKind::Updating
    );
}

#[test]
fn usage_identity_presentation_is_honest_without_account_and_on_failure() {
    let mut view = detail_view(Vec::new(), Some("upstream 503"), UsageSnapshotStatus::Error);
    view.account.account_label.clear();
    let identity = usage_identity_presentation("OpenAI", &view, false);
    assert_eq!(identity.account_label, "No authenticated account");
    assert_eq!(identity.activity_label, "Update failed · Updated 2m ago");
    assert_eq!(
        identity.activity_kind,
        jackin_protocol::control::UsageActivityKind::Exceptional
    );
}

#[test]
fn usage_detail_presentation_flattens_lines_once_in_semantic_order() {
    let mut bucket = presentation_bucket(
        "Weekly",
        Some(40),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.pace_label = Some("5% in deficit · Runs out in 3d 1h".to_owned());
    bucket.reset_label = Some("Resets in 6d 22h".to_owned());
    let view = detail_view(vec![bucket], None, UsageSnapshotStatus::Fresh);
    let presentation = usage_detail_presentation(&view);
    let row = presentation
        .rows
        .iter()
        .find(|r| r.row_id == "bucket:0")
        .expect("bucket row");
    // pace then run-out then reset, exactly once, in that order.
    assert_eq!(
        row.display_label,
        "40% left · 5% in deficit · Runs out in 3d 1h · Resets in 6d 22h"
    );
    // The reset segment is the trailing column; every other segment is leading.
    let flattened: Vec<String> = row
        .layout_lines
        .iter()
        .flat_map(|line| {
            [line.leading.clone(), line.trailing.clone()]
                .into_iter()
                .flatten()
        })
        .collect();
    assert_eq!(
        flattened,
        vec![
            "40% left",
            "5% in deficit",
            "Runs out in 3d 1h",
            "Resets in 6d 22h"
        ]
    );
    let reset_line = row.layout_lines.last().expect("reset line");
    assert_eq!(reset_line.leading, None);
    assert_eq!(reset_line.trailing.as_deref(), Some("Resets in 6d 22h"));
}

#[test]
fn usage_detail_presentation_keeps_duplicate_bucket_labels() {
    let first = presentation_bucket(
        "Weekly",
        Some(80),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    let second = presentation_bucket(
        "Weekly",
        Some(20),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    let view = detail_view(vec![first, second], None, UsageSnapshotStatus::Fresh);
    let presentation = usage_detail_presentation(&view);
    let buckets: Vec<(&str, &str)> = presentation
        .rows
        .iter()
        .filter(|r| r.kind == jackin_protocol::control::UsageDetailRowKind::Bucket)
        .map(|r| (r.row_id.as_str(), r.label.as_str()))
        .collect();
    assert_eq!(
        buckets,
        vec![("bucket:0", "Weekly"), ("bucket:1", "Weekly")]
    );
    let by_id = |id: &str| {
        presentation
            .rows
            .iter()
            .find(|r| r.row_id == id)
            .map(|r| r.display_label.as_str())
    };
    assert_eq!(by_id("bucket:0"), Some("80% left"));
    assert_eq!(by_id("bucket:1"), Some("20% left"));
}

#[test]
fn usage_detail_presentation_stale_keeps_buckets_and_one_detail() {
    let bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Stale,
    );
    let view = detail_view(
        vec![bucket],
        Some("upstream 503"),
        UsageSnapshotStatus::Stale,
    );
    let presentation = usage_detail_presentation(&view);
    let detail_rows: Vec<&jackin_protocol::control::UsageDetailRow> = presentation
        .rows
        .iter()
        .filter(|r| r.kind == jackin_protocol::control::UsageDetailRowKind::Detail)
        .collect();
    assert_eq!(detail_rows.len(), 1, "exactly one Detail row");
    assert_eq!(detail_rows[0].display_label, "upstream 503");
    // The Detail row is last and the last-good bucket survives.
    assert_eq!(
        presentation.rows.last().map(|r| r.row_id.as_str()),
        Some("detail")
    );
    assert!(
        presentation.rows.iter().any(|r| r.row_id == "bucket:0"),
        "bucket retained under stale"
    );
}

#[test]
fn usage_detail_presentation_amp_daily_and_bounds() {
    let mut daily = presentation_bucket(
        "Daily",
        Some(61),
        Some(StatusSlot::Daily),
        UsageSnapshotStatus::Fresh,
    );
    daily.reset_label = Some("Resets daily".to_owned());
    let mut individual = presentation_bucket("Credits", None, None, UsageSnapshotStatus::Fresh);
    individual.limit_label = Some("$4.76".to_owned());
    let view = detail_view(vec![daily, individual], None, UsageSnapshotStatus::Fresh);
    let presentation = usage_detail_presentation(&view);
    let by_id = |id: &str| {
        presentation
            .rows
            .iter()
            .find(|r| r.row_id == id)
            .expect("row")
    };
    let daily_row = by_id("bucket:0");
    assert_eq!(daily_row.display_label, "61% left · Resets daily");
    // No fabricated exact reset timestamp or paid-plan label.
    assert!(!daily_row.display_label.contains('('));
    // Credit bound stays in source order after Daily.
    assert_eq!(by_id("bucket:1").display_label, "$4.76");
}

#[test]
fn usage_detail_presentation_grok_bounds_no_provider_path() {
    let mut weekly = presentation_bucket(
        "Weekly",
        Some(72),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    weekly.reset_label = Some("Resets in 3d".to_owned());
    let mut prepaid = presentation_bucket(
        "Extra usage credits",
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );
    prepaid.limit_label = Some("$25.00".to_owned());
    let view = detail_view(vec![weekly, prepaid], None, UsageSnapshotStatus::Fresh);
    let view = FocusedUsageView {
        account: FocusedAccountHeader {
            plan_label: Some("SuperGrok".to_owned()),
            ..view.account.clone()
        },
        ..view
    };
    let presentation = usage_detail_presentation(&view);
    assert_eq!(
        presentation
            .rows
            .iter()
            .find(|row| row.row_id == "plan")
            .map(|row| row.display_label.as_str()),
        Some("SuperGrok")
    );
    assert_eq!(
        presentation.rows.last().map(|r| r.display_label.as_str()),
        Some("$25.00")
    );
}

#[test]
fn quota_pace_label_appends_runout_when_behind_pace() {
    // time_left=53%, delta=-5; elapsed=470, used=52; 48*470/52=433.85 -> 434s -> "7m"; 434 < 530.
    assert_eq!(
        quota_pace_label(Some(48), Some(10_530), Some(1_000), 10_000).expect("pace"),
        "5% in deficit · Runs out in 7m"
    );
    // Weekly-realistic 7-day window: 48*284401/52 = 262524s ~ 3d; 262524 < 320399.
    assert_eq!(
        quota_pace_label(Some(48), Some(320_399), Some(604_800), 0).expect("pace"),
        "5% in deficit · Runs out in 3d"
    );
}

#[test]
fn quota_pace_label_no_runout_when_ahead_of_pace() {
    // run-out would be 90*400/10 = 3600 >= 600 -> no segment.
    assert_eq!(
        quota_pace_label(Some(90), Some(600), Some(1_000), 0).expect("pace"),
        "30% in reserve"
    );
}

#[test]
fn quota_pace_label_no_runout_when_nothing_used() {
    // used == 0 -> returns without dividing (no division by zero).
    assert_eq!(
        quota_pace_label(Some(100), Some(500), Some(1_000), 0).expect("pace"),
        "50% in reserve"
    );
}

#[test]
fn quota_pace_label_no_runout_at_window_start() {
    // elapsed == 0 -> no segment even though delta = -40.
    assert_eq!(
        quota_pace_label(Some(60), Some(1_000), Some(1_000), 0).expect("pace"),
        "40% in deficit"
    );
}

#[test]
fn quota_pace_label_runout_iff_behind_clock_boundary() {
    // reset_at=500, window=1000, now=0.
    // delta=0 -> On pace; run-out 50*500/50=500, not strictly < 500 -> bare.
    assert_eq!(
        quota_pace_label(Some(50), Some(500), Some(1_000), 0).expect("pace"),
        "On pace"
    );
    // delta=+1 (ahead, in band); 51*500/49=520.4 -> 520 >= 500 -> bare.
    assert_eq!(
        quota_pace_label(Some(51), Some(500), Some(1_000), 0).expect("pace"),
        "On pace"
    );
    // delta=-1 (behind, in band); 49*500/51=480.4 -> 480s -> "8m"; 480 < 500.
    assert_eq!(
        quota_pace_label(Some(49), Some(500), Some(1_000), 0).expect("pace"),
        "On pace · Runs out in 8m"
    );
    // delta=-2 (band edge); 48*500/52=461.5 -> 462s -> "7m".
    assert_eq!(
        quota_pace_label(Some(48), Some(500), Some(1_000), 0).expect("pace"),
        "On pace · Runs out in 7m"
    );
    // delta=-3 (first deficit token); 47*500/53=443.4 -> 443s -> "7m".
    assert_eq!(
        quota_pace_label(Some(47), Some(500), Some(1_000), 0).expect("pace"),
        "3% in deficit · Runs out in 7m"
    );
}

#[test]
fn quota_pace_label_runout_depleted_bucket() {
    // used=100, elapsed=500, run-out=0 < 500 -> trivially precedes reset.
    assert_eq!(
        quota_pace_label(Some(0), Some(500), Some(1_000), 0).expect("pace"),
        "50% in deficit · Runs out in <1m"
    );
}

#[test]
fn quota_pace_label_exact_projection_precedes_reset_before_rounding() {
    // Exact 49*536/51 = 514.98… < 515; display rounding is 515 (would fail if
    // rounded seconds were compared to reset seconds).
    assert_eq!(
        quota_pace_label(Some(49), Some(10_515), Some(1_051), 10_000).expect("pace"),
        "On pace · Runs out in 8m"
    );
}

#[test]
fn quota_pace_label_exact_clock_equality_ignores_float_drift() {
    // 7*1000 == 70*100 -> projection reaches reset exactly -> no run-out segment.
    let label = quota_pace_label(Some(7), Some(70), Some(1_000), 0).expect("pace");
    assert!(!label.contains("Runs out"), "unexpected run-out: {label}");
}
