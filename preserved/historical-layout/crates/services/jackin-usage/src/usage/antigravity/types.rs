// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Antigravity` usage pool and window types.

/// Quota family: Gemini models vs every non-Gemini model (Claude, GPT-OSS, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AntigravityFamily {
    Gemini,
    Other,
}

/// Quota window: the 5-hour session pool vs the weekly pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AntigravityWindow {
    Session,
    Weekly,
}

/// One normalized Antigravity quota pool.
#[derive(Debug, Clone)]
pub(crate) struct AntigravityPool {
    pub(crate) family: AntigravityFamily,
    pub(crate) window: Option<AntigravityWindow>,
    /// Remaining percent. `None` only when the source carried no quota signal
    /// at all for a pool whose identity is known (kept distinct from 0, which
    /// is a genuinely depleted pool).
    pub(crate) remaining_percent: Option<u8>,
    pub(crate) reset_at: Option<i64>,
    /// Source label for pools whose window could not be determined (kept as a
    /// detail row under their own id rather than dropped or mis-slotted).
    pub(crate) source_label: Option<String>,
}

/// Parsed `/usage` output: quota pools plus optional identity/plan.
#[derive(Debug, Clone, Default)]
pub(crate) struct AntigravityUsage {
    pub(crate) pools: Vec<AntigravityPool>,
    pub(crate) identity: Option<String>,
    pub(crate) plan: Option<String>,
    /// True when pools came from the legacy per-model shape (5h-only); weekly
    /// buckets then render "No data" instead of invented allowance.
    pub(crate) legacy_fallback: bool,
}
