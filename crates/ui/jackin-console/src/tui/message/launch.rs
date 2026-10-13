// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Launch-agent prompt plans and dispatch.

use super::{AgentPickerResolution, LaunchAgentPromptPlan};

#[must_use]
pub fn launch_agent_prompt_plan(
    resolution: AgentPickerResolution,
    on_failure: OnPromptFailure,
) -> LaunchAgentPromptPlan {
    match resolution {
        AgentPickerResolution::Opened => LaunchAgentPromptPlan {
            outcome: PromptOutcome::Defer,
            store_pending_launch: true,
            error: None,
        },
        AgentPickerResolution::NotNeeded => LaunchAgentPromptPlan {
            outcome: PromptOutcome::Launch,
            store_pending_launch: false,
            error: None,
        },
        AgentPickerResolution::Failed(error) => LaunchAgentPromptPlan {
            outcome: PromptOutcome::Defer,
            store_pending_launch: matches!(on_failure, OnPromptFailure::RestorePending),
            error: Some(error),
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptOutcome {
    Launch,
    Defer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnPromptFailure {
    ClearPending,
    RestorePending,
}

#[derive(Debug)]
pub enum LaunchPromptDispatch<Outcome, Request> {
    Launch(Outcome),
    Prompt(Request),
    None,
}

#[derive(Debug)]
pub struct LaunchPromptRequest<Role, Workspace, Input> {
    pub role: Role,
    pub workspace: Workspace,
    pub input: Input,
    pub on_failure: OnPromptFailure,
}
