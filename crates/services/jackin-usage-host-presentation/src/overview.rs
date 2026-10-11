// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Host overview and glance row types.

/// One enabled-surface overview row for jackin❯ desktop (popover + Usage window).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostOverviewRow {
    /// Machine surface id (`claude`, `codex`, …).
    pub surface_id: String,
    /// Remapped display label (`OpenAI`, `Anthropic`, …).
    pub display_label: String,
    /// Percent headline or empty when only a status word applies.
    pub headline: String,
    /// Countdown-form reset line when known.
    pub reset_label: Option<String>,
    /// Exact clock parenthetical when `resets_at` is known, e.g. `(Jul 28, 17:02)`.
    pub exact_reset: Option<String>,
    /// Storage status word (`fresh`, `stale`, `needs_login`, …).
    pub status_word: String,
    /// Worst bucket severity: `normal` | `warn` | `danger`.
    pub severity: String,
}

/// One selected-account-aware provider projection for native usage surfaces
/// (the Desktop status bar, popover, and Usage window all consume this same
/// Rust-owned row rather than choosing providers or formatting quota in Swift).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostProviderGlanceRow {
    /// Stable provider machine identifier (`codex`, `claude`, …).
    pub surface_id: String,
    /// Stable provider icon key (closed domain, equals `surface_id`).
    pub icon_key: String,
    /// Rust-owned fallback glyph.
    pub fallback_glyph: String,
    /// Provider usage/settings URL.
    pub usage_url: Option<String>,
    /// Rust-owned provider display name (`OpenAI`, `Anthropic`, …).
    pub display_label: String,
    /// Rust-owned selected-account label (empty when none).
    pub account_label: String,
    /// Provider plan label when known.
    pub plan_label: Option<String>,
    /// Selected semantic glance percentage (Weekly for six, Daily for Amp),
    /// when the required bucket exists.
    pub glance_remaining_percent: Option<u8>,
    /// Verbatim menu-bar value (`57%` or `–`).
    pub bar_label: String,
    /// Verbatim detail headline (`57% left` or `–`).
    pub headline: String,
    /// Relative reset label when the glance bucket carries a reset.
    pub reset_label: Option<String>,
    /// Compact countdown token used by the menu-bar chip (`<1m`, `2h 14m`).
    pub compact_reset_label: Option<String>,
    /// Exact-clock reset parenthetical when the glance bucket carries a reset.
    pub exact_reset: Option<String>,
    /// Stable machine status word.
    pub status_word: String,
    /// Whether this provider is the cold refreshing placeholder.
    pub is_refreshing: bool,
    /// Rust-owned human status label.
    pub status_label: String,
    /// Stable presentation-severity key (`normal` | `warn` | `danger`).
    pub severity: String,
    /// Rust-owned freshness label.
    pub updated_label: String,
    /// The single Rust-owned activity phrase for this selected provider/account.
    pub activity_label: String,
    /// Machine activity kind (`idle` | `updating` | `exceptional`).
    pub activity_kind: String,
    /// Complete menu-bar/popover accessibility and tooltip copy.
    pub accessibility_label: String,
    /// Rust-owned last error, when present.
    pub last_error: Option<String>,
    /// Whether the native bar value is visually dimmed (stale/error).
    pub dimmed: bool,
}
