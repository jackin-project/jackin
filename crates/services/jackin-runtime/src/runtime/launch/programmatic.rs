// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Programmatic (non-TTY) launch surface for [`super::LoadOptions`].
//!
//! The interactive CLI resolves every launch decision through dialogs: the
//! agent picker, the trust prompt, the sensitive-mount confirmation, the
//! on-demand credential picker. A daemon has no terminal to answer any of
//! them, so a programmatic launch must arrive with every decision already
//! made and be *rejected up front* when one is missing — never fall through
//! to a dialog that cannot be drawn.
//!
//! The options bag ([`super::LoadOptions`]), its validation
//! ([`super::LoadOptions::validate_programmatic`]), and the identity a
//! launch reports back ([`LaunchedInstance`]) moved to
//! `jackin_runtime_launch_load_options::load_options` (S7 split 87) and are
//! re-exported below; this module keeps the agent-specific model/effort env
//! mapping. The launch itself keeps running
//! through the one shared pipeline the CLI uses — nothing here forks it.

use jackin_core::{Agent, ReasoningEffort};

// Test-only scope restoration: the hub suite below names these through
// `use super::*`; prod no longer needs them after split 87 moved the
// options bag and its validation into the load-options leaf.
#[cfg(test)]
use jackin_config::{AgentConfiguration, AppConfig};
#[cfg(test)]
use jackin_core::RoleSelector;

// Moved to jackin_runtime_launch_load_options::load_options (S7 split 87);
// the item re-exports keep every `programmatic::*` path stable.
pub use jackin_runtime_launch_load_options::load_options::{
    IdentitySink, LaunchedInstance, LoadOptionsError,
};

/// Env var carrying the Codex model to the in-container role hook (`SCHED-014`).
pub const CODEX_LANE_MODEL_ENV: &str = jackin_core::CODEX_LANE_MODEL_ENV_NAME;
/// Env var carrying the Codex reasoning effort to the same role hook.
pub const CODEX_LANE_EFFORT_ENV: &str = jackin_core::CODEX_LANE_EFFORT_ENV_NAME;
/// Env var Claude Code reads for its model.
pub const CLAUDE_MODEL_ENV: &str = jackin_core::CLAUDE_MODEL_ENV_NAME;
/// Env var Claude Code reads for its reasoning effort.
pub const CLAUDE_EFFORT_ENV: &str = jackin_core::CLAUDE_EFFORT_ENV_NAME;

/// Container env that pins `model` and reasoning effort for `agent`.
///
/// Codex reads neither from its argv: the sourced role hook writes `model` and
/// `model_reasoning_effort` into `$CODEX_HOME/config.toml` from these two
/// variables. Passing the launch's model through the same pair — while the
/// capsule also receives it as the agent's model — is what keeps the hook and
/// the daemon from disagreeing about which model is running (D-078).
///
/// Returns entries in a stable order so the launch env is reproducible.
#[must_use]
pub fn lane_agent_env(
    agent: Agent,
    model: Option<&str>,
    effort: Option<ReasoningEffort>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let (model_key, effort_key) = match agent {
        Agent::Codex => (CODEX_LANE_MODEL_ENV, CODEX_LANE_EFFORT_ENV),
        Agent::Claude => (CLAUDE_MODEL_ENV, CLAUDE_EFFORT_ENV),
        // Every other runtime takes its model on argv (the capsule passes
        // `-m`/`--model`), and declares no effort knob today.
        _ => return out,
    };
    if let Some(model) = model.map(str::trim).filter(|m| !m.is_empty()) {
        out.push((model_key.to_owned(), model.to_owned()));
    }
    if let Some(effort) = effort {
        out.push((effort_key.to_owned(), effort.as_str().to_owned()));
    }
    out
}

// Moved to jackin_runtime_launch_programmatic_selection::selection (S7
// split 86); the item re-exports keep every `programmatic::*` path stable.
pub use jackin_runtime_launch_programmatic_selection::selection::{
    with_account_selection, with_configuration_selection,
};

#[cfg(test)]
mod tests;
