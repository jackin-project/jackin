// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Claude usage diagnostics and freshness labels.

use std::time::{SystemTime, UNIX_EPOCH};

use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};

/// `Auth:` origin label for an OAuth credential resolved from `path`, with the
/// home dir collapsed to `~` (so it reads `~/.codex/auth.json`, not an absolute
/// container path). Shared by the Claude and Codex snapshots.
pub fn now_epoch() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

pub fn relative_updated_label(fetched_at: i64, now_epoch: i64) -> String {
    let age = now_epoch.saturating_sub(fetched_at).max(0);
    if age < 60 {
        "Updated now".to_owned()
    } else if age < 3_600 {
        format!("Updated {}m ago", age / 60)
    } else {
        format!("Updated {}h ago", age / 3_600)
    }
}

pub fn refresh_cached_updated_label(view: &mut FocusedUsageView, now_epoch: i64) {
    if matches!(
        view.status,
        UsageSnapshotStatus::Fresh | UsageSnapshotStatus::Stale
    ) || view.updated_label.trim().is_empty()
    {
        view.updated_label = relative_updated_label(view.fetched_at_epoch, now_epoch);
    }
}
