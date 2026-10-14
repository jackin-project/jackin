// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostOverviewRow {
    pub surface_id: String,

    pub display_label: String,

    pub headline: String,

    pub reset_label: Option<String>,

    pub exact_reset: Option<String>,

    pub status_word: String,

    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostProviderGlanceRow {
    pub surface_id: String,

    pub icon_key: String,

    pub fallback_glyph: String,

    pub usage_url: Option<String>,

    pub display_label: String,

    pub account_label: String,

    pub plan_label: Option<String>,

    pub glance_remaining_percent: Option<u8>,

    pub bar_label: String,

    pub headline: String,

    pub reset_label: Option<String>,

    pub compact_reset_label: Option<String>,

    pub exact_reset: Option<String>,

    pub status_word: String,

    pub is_refreshing: bool,

    pub status_label: String,

    pub severity: String,

    pub updated_label: String,

    pub activity_label: String,

    pub activity_kind: String,

    pub accessibility_label: String,

    pub last_error: Option<String>,

    pub dimmed: bool,
}
