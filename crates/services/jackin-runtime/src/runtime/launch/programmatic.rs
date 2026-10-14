// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use jackin_config::{AgentConfiguration, AppConfig};
#[cfg(test)]
use jackin_core::{Agent, ReasoningEffort, RoleSelector};

pub use jackin_runtime_launch_load_options::lane_env::{
    CLAUDE_EFFORT_ENV, CLAUDE_MODEL_ENV, CODEX_LANE_EFFORT_ENV, CODEX_LANE_MODEL_ENV,
    lane_agent_env,
};
pub use jackin_runtime_launch_load_options::load_options::{
    IdentitySink, LaunchedInstance, LoadOptionsError,
};

pub use jackin_runtime_launch_programmatic_selection::selection::{
    with_account_selection, with_configuration_selection,
};

#[cfg(test)]
mod tests;
