// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_protocol::control::PercentObservationOrigin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityFamily {
    Gemini,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntigravityWindow {
    Session,
    Weekly,
}

#[derive(Debug, Clone)]
pub(crate) struct AntigravityUtilizationObservation {
    pub(crate) source: String,
    pub(crate) value: serde_json::Number,
    pub(crate) reset_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AntigravityPool {
    pub(crate) family: AntigravityFamily,
    pub(crate) window: Option<AntigravityWindow>,
    pub(crate) used_percent: Option<f64>,
    pub(crate) remaining_percent: Option<f64>,
    pub(crate) used_percent_origin: Option<PercentObservationOrigin>,
    pub(crate) remaining_percent_origin: Option<PercentObservationOrigin>,
    pub(crate) reset_at: Option<i64>,
    pub(crate) source_label: Option<String>,
    pub(crate) unclassified_utilizations: Vec<AntigravityUtilizationObservation>,
}

#[derive(Debug, Clone, Default)]
pub struct AntigravityUsage {
    pub(crate) pools: Vec<AntigravityPool>,
    pub(crate) identity: Option<String>,
    pub(crate) plan: Option<String>,
    pub(crate) legacy_fallback: bool,
}
