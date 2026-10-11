// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Agent and role prompting helpers for the workspace manager event loop.
mod accounts;
mod agent_default;
mod launch;
#[cfg(test)]
mod tests;
pub use crate::tui::message::{AgentPickerChoices, LaunchPromptDispatch, LaunchPromptRequest};
pub use accounts::{
    LaunchAccountSelection, no_admitted_instance_message, no_eligible_account_message,
    select_launch_account, sort_account_choices_by_id,
};
pub use agent_default::{AgentDefaultResolution, resolve_agent_default};
pub use launch::{
    ConcreteAgentPickerChoices, ConcreteLaunchPromptDispatch, ConcreteLaunchPromptRequest,
    committed_role_prompt, dispatch_launch_prompt, draw_role_resolution_dialog,
    launch_with_committed_agent, prompt_agent_for_launch, show_role_resolution_error,
};

#[cfg(test)]
pub(crate) use jackin_config::AppConfig;
