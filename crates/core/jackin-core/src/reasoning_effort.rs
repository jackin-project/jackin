// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! `ReasoningEffort`: the per-launch reasoning-effort knob shared by every
//! agent runtime.
//!
//! Effort is a launch decision, not a role property: the same role runs at a
//! different effort depending on the lane that launched it. The vocabulary is
//! closed (`low` / `medium` / `high` / `max`) so a caller cannot smuggle an
//! agent-specific spelling through the programmatic launch surface — each
//! runtime maps the closed set onto its own knob at launch time.

use std::fmt;
use std::str::FromStr;

/// Reasoning effort requested for an agent session.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReasoningEffort {
    /// Fastest, least deliberate setting.
    Low,
    /// Default setting.
    #[default]
    Medium,
    /// Slowest, most deliberate setting.
    High,
    /// Maximum supported reasoning setting.
    Max,
}

impl ReasoningEffort {
    /// Every effort level, in ascending order.
    pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::Max];

    /// Canonical lowercase spelling, which is also the value written to
    /// `model_reasoning_effort` in a Codex `config.toml` and to
    /// `CLAUDE_CODE_EFFORT_LEVEL`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Rejected spelling of a reasoning-effort level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseReasoningEffortError {
    /// The rejected input, verbatim.
    pub input: String,
}

impl fmt::Display for ParseReasoningEffortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown reasoning effort {:?}; expected one of: low, medium, high, max",
            self.input
        )
    }
}

impl std::error::Error for ParseReasoningEffortError {}

impl FromStr for ReasoningEffort {
    type Err = ParseReasoningEffortError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "max" => Ok(Self::Max),
            _ => Err(ParseReasoningEffortError {
                input: s.to_owned(),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
