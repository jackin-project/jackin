// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};

use crate::{CachedUsage, account_snapshot_views_from_cache, bucket};

#[test]
fn account_snapshot_rows_preserve_snapshot_status_with_retained_fresh_bucket() {
    for (input_status, expected_status) in [
        (UsageSnapshotStatus::Fresh, "fresh"),
        (UsageSnapshotStatus::Stale, "stale"),
        (UsageSnapshotStatus::Error, "error"),
    ] {
        let mut view = FocusedUsageView::unavailable("fixture account", 1);
        view.status = input_status;
        view.last_error = None;
        view.buckets.push(bucket(
            "primary",
            None,
            None,
            None,
            None,
            None,
            UsageSnapshotStatus::Fresh,
        ));
        let snapshots = HashMap::from([("fixture".to_owned(), CachedUsage { view })]);

        assert_eq!(
            snapshots["fixture"].view.buckets[0].status,
            UsageSnapshotStatus::Fresh
        );
        let rows = account_snapshot_views_from_cache(&snapshots);
        assert_eq!(rows.len(), 1, "input status: {input_status:?}");
        assert_eq!(
            rows[0].status, expected_status,
            "input status: {input_status:?}"
        );
    }
}
