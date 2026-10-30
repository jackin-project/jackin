// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::UsageSnapshotStatus;
use jackin_protocol::usage_broker::{UsageFreshnessPhaseV1, UsageQuotaStateV1};
use jackin_usage_provider_core::bucket;

use crate::{freshness, quota_state_for_bucket, status_label};

#[test]
fn status_labels_cover_every_snapshot_state() {
    assert_eq!(status_label(UsageSnapshotStatus::Fresh), "Available");
    assert_eq!(status_label(UsageSnapshotStatus::Stale), "Stale");
    assert_eq!(status_label(UsageSnapshotStatus::NeedsLogin), "Needs login");
    assert_eq!(
        status_label(UsageSnapshotStatus::NeedsSecret),
        "Needs secret"
    );
    assert_eq!(
        status_label(UsageSnapshotStatus::Unsupported),
        "Unsupported"
    );
    assert_eq!(
        status_label(UsageSnapshotStatus::Unavailable),
        "Unavailable"
    );
    assert_eq!(status_label(UsageSnapshotStatus::Error), "Error");
}

#[test]
fn fresh_snapshot_marks_current_generation_with_last_good() {
    let freshness = freshness(UsageSnapshotStatus::Fresh, 42, 7);

    assert_eq!(freshness.phase, UsageFreshnessPhaseV1::Current);
    assert_eq!(freshness.generation, 7);
    assert_eq!(freshness.last_good_at_epoch, Some(42));
    assert!(!freshness.is_stale);
}

#[test]
fn error_snapshot_marks_failed_generation_without_last_good() {
    let freshness = freshness(UsageSnapshotStatus::Error, 42, 7);

    assert_eq!(freshness.phase, UsageFreshnessPhaseV1::Failed);
    assert_eq!(freshness.generation, 7);
    assert_eq!(freshness.last_good_at_epoch, None);
    assert!(!freshness.is_stale);
}

#[test]
fn stale_snapshot_marks_stale_generation_with_last_good() {
    let freshness = freshness(UsageSnapshotStatus::Stale, 42, 7);

    assert_eq!(freshness.phase, UsageFreshnessPhaseV1::Stale);
    assert_eq!(freshness.last_good_at_epoch, Some(42));
    assert!(freshness.is_stale);
}

#[test]
fn fresh_empty_bucket_reports_unknown_quota_state() {
    let bucket = bucket(
        "Quota",
        None,
        None,
        None,
        None,
        None,
        UsageSnapshotStatus::Fresh,
    );

    assert_eq!(quota_state_for_bucket(&bucket), UsageQuotaStateV1::Unknown);
}
