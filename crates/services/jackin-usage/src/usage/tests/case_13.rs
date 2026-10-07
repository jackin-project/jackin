// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn usage_bucket_presentation_orders_normal_segments() {
    let mut bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.pace_label = Some("13% in deficit · Runs out in 2d".to_owned());
    bucket.reset_label = Some("Resets in 4d".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec![
            "57% left",
            "13% in deficit",
            "Runs out in 2d",
            "Resets in 4d"
        ]
    );
    assert_eq!(presentation.remaining_label.as_deref(), Some("57% left"));
    assert_eq!(presentation.meter_percent, Some(57));
    assert_eq!(
        presentation.display_label,
        "57% left · 13% in deficit · Runs out in 2d · Resets in 4d"
    );
}

#[test]
fn usage_bucket_presentation_flattens_runout_composite() {
    let mut bucket = presentation_bucket(
        "Weekly",
        Some(40),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.pace_label = Some("On pace · Runs out in 5d".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec!["40% left", "On pace", "Runs out in 5d"]
    );
}

#[test]
fn usage_bucket_presentation_orders_spend_cap() {
    let mut bucket = presentation_bucket(
        "Extra usage",
        Some(70),
        Some(StatusSlot::Spend),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("SGD 78.49".to_owned());
    bucket.limit_label = Some("SGD 260.00".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments,
        vec!["30% used", "Monthly cap: SGD 78.49 / SGD 260.00"]
    );
    // Spend text reads used, but meter geometry fills by remaining — the same
    // rule as every other slot and the console windows.
    assert_eq!(presentation.meter_percent, Some(70));
}

#[test]
fn usage_bucket_presentation_recovers_spend_overage_from_money() {
    // $150 against a $100 cap with a saturated 0% remaining: the money ratio
    // recovers the raw "150% used" text (matching the console window value)
    // and the meter reads empty (nothing left), matching the console meter.
    let mut bucket = presentation_bucket(
        "Extra usage",
        Some(0),
        Some(StatusSlot::Spend),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("$150.00".to_owned());
    bucket.limit_label = Some("$100.00".to_owned());
    bucket.used_money = Some(Money::new(15_000, "USD", 2));
    bucket.limit_money = Some(Money::new(10_000, "USD", 2));
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.remaining_label.as_deref(), Some("150% used"));
    assert_eq!(presentation.meter_percent, Some(0));

    // A non-overage money ratio agrees with the remaining percent; the text
    // still reads used and the meter still fills by remaining.
    let mut bucket = presentation_bucket(
        "Extra usage",
        Some(55),
        Some(StatusSlot::Spend),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("$45.20".to_owned());
    bucket.limit_label = Some("$100.00".to_owned());
    bucket.used_money = Some(Money::new(4_520, "USD", 2));
    bucket.limit_money = Some(Money::new(10_000, "USD", 2));
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.remaining_label.as_deref(), Some("45% used"));
    assert_eq!(presentation.meter_percent, Some(55));
}

#[test]
fn usage_bucket_presentation_orders_non_spend_budget() {
    let mut bucket = presentation_bucket(
        "Global budget",
        None,
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Fresh,
    );
    bucket.used_label = Some("$0.00 spent".to_owned());
    bucket.limit_label = Some("$25,000.00".to_owned());
    bucket.used_money = Some(Money::new(0, "USD", 2));
    bucket.limit_money = Some(Money::new(2_500_000, "USD", 2));
    let presentation = usage_bucket_presentation(&bucket);
    assert!(
        presentation
            .display_segments
            .contains(&"Budget: $0.00 spent / $25,000.00".to_owned())
    );
}

#[test]
fn usage_bucket_presentation_appends_degraded_status() {
    let bucket = presentation_bucket(
        "Weekly",
        Some(57),
        Some(StatusSlot::Weekly),
        UsageSnapshotStatus::Stale,
    );
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.display_segments, vec!["57% left", "stale"]);
}

#[test]
fn usage_bucket_presentation_credits_zero_left() {
    let mut bucket = presentation_bucket("Credits", Some(0), None, UsageSnapshotStatus::Fresh);
    bucket.limit_label = Some("$4.76".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(
        presentation.display_segments.first().map(String::as_str),
        Some("0 left")
    );
    assert!(presentation.display_segments.contains(&"$4.76".to_owned()));
    assert_eq!(presentation.meter_percent, Some(0));
}

#[test]
fn usage_bucket_presentation_limit_only_balance() {
    let mut bucket = presentation_bucket("Prepaid", None, None, UsageSnapshotStatus::Fresh);
    bucket.limit_label = Some("$25".to_owned());
    let presentation = usage_bucket_presentation(&bucket);
    assert_eq!(presentation.display_segments, vec!["$25"]);
    assert_eq!(presentation.meter_percent, None);
    assert_eq!(presentation.remaining_label, None);
}

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
