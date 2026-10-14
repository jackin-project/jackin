// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::{QuotaBucketView, StatusSlot, UsageSeverity, UsageSnapshotStatus};
use jackin_usage_provider_core::{
    json_number, parse_iso_epoch, quota_pace_label, remaining_from_fraction, severity_from_label,
    timed_bucket, used_percent_label,
};

use super::{
    ClaudeOAuthLimit, ClaudeOAuthUsageResponse, ClaudeOAuthUsageWindow, claude_spend_bucket,
    push_claude_dollar_windows,
};

pub const CLAUDE_SESSION_WINDOW_SECONDS: i64 = 5 * 60 * 60;

pub const CLAUDE_WEEKLY_WINDOW_SECONDS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct ClaudeQuotaWindow {
    pub label: String,
    pub slot: Option<StatusSlot>,

    pub used: Option<f64>,
    pub reset_at: Option<i64>,
    pub window_seconds: Option<i64>,
    pub severity: UsageSeverity,
}

impl ClaudeQuotaWindow {
    pub fn into_bucket(self, now: i64) -> QuotaBucketView {
        let remaining = self.used.and_then(remaining_from_fraction);
        let pace = quota_pace_label(remaining, self.reset_at, self.window_seconds, now);
        let mut view = timed_bucket(
            &self.label,
            self.used.and_then(used_percent_label),
            Some("100%".to_owned()),
            remaining,
            self.reset_at,
            now,
            pace.as_deref(),
            UsageSnapshotStatus::Fresh,
        );
        view.status_slot = self.slot;
        view.severity = self.severity;
        view
    }
}

impl ClaudeOAuthUsageWindow {
    pub fn into_quota(
        self,
        label: &str,
        slot: Option<StatusSlot>,
        window_seconds: Option<i64>,
    ) -> ClaudeQuotaWindow {
        ClaudeQuotaWindow {
            label: label.to_owned(),
            slot,
            used: self.utilization,
            reset_at: self.resets_at.as_deref().and_then(parse_iso_epoch),
            window_seconds,

            severity: UsageSeverity::Normal,
        }
    }
}

impl ClaudeOAuthLimit {
    pub fn as_quota(&self) -> Option<ClaudeQuotaWindow> {
        let percent = json_number(self.percent.as_ref()?)?;
        let (label, slot, window_seconds) = match self.kind.as_deref()? {
            "session" => (
                "Session".to_owned(),
                Some(StatusSlot::Session),
                Some(CLAUDE_SESSION_WINDOW_SECONDS),
            ),
            "weekly_all" => (
                "All models".to_owned(),
                Some(StatusSlot::Weekly),
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ),
            "weekly_scoped" => (
                self.scoped_label()?,
                None,
                Some(CLAUDE_WEEKLY_WINDOW_SECONDS),
            ),
            _ => return None,
        };
        Some(ClaudeQuotaWindow {
            label,
            slot,
            used: Some(percent),
            reset_at: self.resets_at.as_deref().and_then(parse_iso_epoch),
            window_seconds,
            severity: severity_from_label(self.severity.as_deref()),
        })
    }

    pub fn scoped_label(&self) -> Option<String> {
        self.scope
            .as_ref()
            .and_then(|scope| scope.model.as_ref())
            .and_then(|model| model.display_name.as_deref())
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_owned)
    }
}

impl ClaudeOAuthUsageResponse {
    pub fn into_buckets(self, now: i64) -> Vec<QuotaBucketView> {
        let Self {
            five_hour,
            seven_day,
            seven_day_sonnet,
            seven_day_opus,
            seven_day_routines,
            limits,
            extra_usage,
            spend,
            other_windows,
        } = self;

        let mut windows: Vec<ClaudeQuotaWindow> = limits
            .iter()
            .filter_map(ClaudeOAuthLimit::as_quota)
            .collect();
        for window in legacy_claude_quota_windows(
            five_hour,
            seven_day,
            seven_day_sonnet,
            seven_day_opus,
            seven_day_routines,
        ) {
            if !has_equivalent_claude_window(&windows, &window) {
                windows.push(window);
            }
        }
        let mut buckets: Vec<QuotaBucketView> =
            windows.into_iter().map(|w| w.into_bucket(now)).collect();
        if let Some(spend) = claude_spend_bucket(spend, extra_usage) {
            buckets.push(spend);
        }
        push_claude_dollar_windows(&mut buckets, other_windows, now);
        buckets
    }
}

fn has_equivalent_claude_window(
    windows: &[ClaudeQuotaWindow],
    candidate: &ClaudeQuotaWindow,
) -> bool {
    match candidate.slot {
        Some(StatusSlot::Session) => windows
            .iter()
            .any(|window| window.slot == Some(StatusSlot::Session)),
        Some(StatusSlot::Weekly) => windows
            .iter()
            .any(|window| window.slot == Some(StatusSlot::Weekly)),
        _ => windows.iter().any(|window| {
            window.slot.is_none() && window.label.eq_ignore_ascii_case(&candidate.label)
        }),
    }
}

fn legacy_claude_quota_windows(
    five_hour: Option<ClaudeOAuthUsageWindow>,
    seven_day: Option<ClaudeOAuthUsageWindow>,
    seven_day_sonnet: Option<ClaudeOAuthUsageWindow>,
    seven_day_opus: Option<ClaudeOAuthUsageWindow>,
    seven_day_routines: Option<ClaudeOAuthUsageWindow>,
) -> Vec<ClaudeQuotaWindow> {
    let session = Some(CLAUDE_SESSION_WINDOW_SECONDS);
    let weekly = Some(CLAUDE_WEEKLY_WINDOW_SECONDS);
    let mut windows = Vec::new();
    if let Some(window) = five_hour {
        windows.push(window.into_quota("Session", Some(StatusSlot::Session), session));
    }
    if let Some(window) = seven_day {
        windows.push(window.into_quota("Weekly", Some(StatusSlot::Weekly), weekly));
    }
    if let Some(window) = seven_day_sonnet {
        windows.push(window.into_quota("Sonnet", None, weekly));
    }
    if let Some(window) = seven_day_opus {
        windows.push(window.into_quota("Opus", None, weekly));
    }
    if let Some(window) = seven_day_routines {
        windows.push(window.into_quota("Daily Routines", None, weekly));
    }
    windows
}
