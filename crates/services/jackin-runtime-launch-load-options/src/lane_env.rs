// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::{Agent, ReasoningEffort};

pub const CODEX_LANE_MODEL_ENV: &str = jackin_core::CODEX_LANE_MODEL_ENV_NAME;
pub const CODEX_LANE_EFFORT_ENV: &str = jackin_core::CODEX_LANE_EFFORT_ENV_NAME;
pub const CLAUDE_MODEL_ENV: &str = jackin_core::CLAUDE_MODEL_ENV_NAME;
pub const CLAUDE_EFFORT_ENV: &str = jackin_core::CLAUDE_EFFORT_ENV_NAME;

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
