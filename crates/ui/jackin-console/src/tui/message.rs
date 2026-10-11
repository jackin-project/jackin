// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Top-level console TUI message helpers.
//!
//! Product-specific manager messages still live in the root crate while the
//! workspace console owns root-only config/runtime types. Generic message
//! carriers live here so the top-level TUI vocabulary has a home in the
//! surface crate.
mod actions;
mod launch;
mod manager;
mod modals;
mod refresh;
#[cfg(test)]
mod tests;
pub use actions::{
    BackgroundEvent, ConsoleInputOutcome, ConsoleInstanceAction, ConsoleOutcome,
    InstanceActionHandler,
};
pub use launch::{
    LaunchPromptDispatch, LaunchPromptRequest, OnPromptFailure, PromptOutcome,
    launch_agent_prompt_plan,
};
pub use manager::ConsoleManagerMessage;
pub use modals::{
    AgentPickerChoices, AgentPickerResolution, ConsoleEditorModalOutcome,
    ConsolePreludeModalOutcome, ConsoleSettingsAuthOutcome, ConsoleSettingsModalOutcome,
    LaunchAgentPromptPlan, agent_picker_choices_for_workspace, launch_prompt_should_probe_agents,
};
pub use refresh::{
    MountInfoRefreshSourceFacts, MountInfoRefreshSourcePlan, MountInfoRefreshTarget,
    PendingMountInfoRefresh, mount_info_refresh_source_plan,
};
