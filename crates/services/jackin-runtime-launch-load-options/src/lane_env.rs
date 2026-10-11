// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Model/effort container env for a launched agent ([`lane_agent_env`]).
//!
//! Moved out of `jackin-runtime` `launch/programmatic.rs` (S7 split 88)
//! with the rest of the options companions; `programmatic.rs` keeps the
//! `jackin_runtime::runtime::launch::programmatic::*` paths stable through
//! item re-exports.

use jackin_core::{Agent, ReasoningEffort};

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
