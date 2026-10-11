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
//! ([`super::LoadOptions::validate_programmatic`]), the identity a
//! launch reports back ([`LaunchedInstance`]), and the agent-specific
//! model/effort env mapping moved to
//! `jackin_runtime_launch_load_options::load_options` (S7 splits 87-88)
//! and are re-exported below. The launch itself keeps running
//! through the one shared pipeline the CLI uses — nothing here forks it.

// Test-only scope restoration: the hub suite below names these through
// `use super::*`; prod is re-exports only after splits 87-88 moved the
// options bag, its validation, and the env mapping into the leaf.
#[cfg(test)]
use jackin_config::{AgentConfiguration, AppConfig};
#[cfg(test)]
use jackin_core::{Agent, ReasoningEffort, RoleSelector};

// Moved to jackin_runtime_launch_load_options (S7 splits 87-88); the item
// re-exports keep every `programmatic::*` path stable.
pub use jackin_runtime_launch_load_options::lane_env::{
    CLAUDE_EFFORT_ENV, CLAUDE_MODEL_ENV, CODEX_LANE_EFFORT_ENV, CODEX_LANE_MODEL_ENV,
    lane_agent_env,
};
pub use jackin_runtime_launch_load_options::load_options::{
    IdentitySink, LaunchedInstance, LoadOptionsError,
};

// Moved to jackin_runtime_launch_programmatic_selection::selection (S7
// split 86); the item re-exports keep every `programmatic::*` path stable.
pub use jackin_runtime_launch_programmatic_selection::selection::{
    with_account_selection, with_configuration_selection,
};

#[cfg(test)]
mod tests;
