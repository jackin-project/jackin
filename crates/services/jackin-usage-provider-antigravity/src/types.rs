// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
pub struct AntigravityPool {
    pub(crate) family: AntigravityFamily,
    pub(crate) window: Option<AntigravityWindow>,

    pub(crate) remaining_percent: Option<u8>,
    pub(crate) reset_at: Option<i64>,

    pub(crate) source_label: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AntigravityUsage {
    pub(crate) pools: Vec<AntigravityPool>,
    pub(crate) identity: Option<String>,
    pub(crate) plan: Option<String>,

    pub(crate) legacy_fallback: bool,
}
