// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn placeholder_sample_bucket() -> QuotaBucketView {
    QuotaBucketView {
        label: "Weekly".to_owned(),
        used_label: None,
        limit_label: None,
        remaining_percent: Some(57),
        reset_label: None,
        resets_at: None,
        status_slot: Some(StatusSlot::Weekly),
        pace_label: None,
        status: UsageSnapshotStatus::Fresh,
        used_money: None,
        limit_money: None,
        severity: UsageSeverity::Normal,
    }
}
