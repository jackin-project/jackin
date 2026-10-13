// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Screen modal outcome carriers.

use super::PromptOutcome;
use std::path::PathBuf;

#[derive(Debug)]
pub enum ConsolePreludeModalOutcome {
    Continue,
    OpenUrl(String),
    ReopenFileBrowserAtLastCwd,
    ApplyFileBrowserOutcome {
        outcome: crate::tui::components::file_browser::FileBrowserOutcome<PathBuf>,
        browser_cwd: Option<PathBuf>,
    },
    ResolveFileBrowserGitUrl(PathBuf),
}

#[derive(Debug)]
pub enum ConsoleEditorModalOutcome<RoleSelector, RoleSource, OpRef> {
    Continue,
    StartRoleRegistration {
        raw: String,
        key: String,
        selector: RoleSelector,
        source: RoleSource,
    },
    PersistTrustedRoleSource {
        key: String,
        source: RoleSource,
    },
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<PathBuf>),
    ResolveFileBrowserGitUrl(PathBuf),
    OpenAuthSourceFolderBrowser,
    OpenUrl(String),
    ValidateOpRef(OpRef),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConsoleSettingsModalOutcome {
    Continue,
    SaveSettings,
    OpenGlobalMountFileBrowser,
    OpenUrl(String),
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<PathBuf>),
    ResolveFileBrowserGitUrl(PathBuf),
}

#[derive(Debug)]
pub enum ConsoleSettingsAuthOutcome<OpRef> {
    Continue,
    OpenAuthSourceFolderBrowser,
    ApplyFileBrowserOutcome(crate::tui::components::file_browser::FileBrowserOutcome<PathBuf>),
    ValidateOpRef(OpRef),
}

#[derive(Debug)]
pub enum AgentPickerResolution {
    Opened,
    NotNeeded,
    Failed(anyhow::Error),
}

#[derive(Debug)]
pub enum AgentPickerChoices<Agent> {
    Choices(Vec<Agent>),
    NotNeeded,
    Failed(anyhow::Error),
}

#[must_use]
pub fn agent_picker_choices_for_workspace<Agent>(
    default_agent_configured: bool,
    choices: AgentPickerChoices<Agent>,
) -> AgentPickerChoices<Agent> {
    if default_agent_configured {
        AgentPickerChoices::NotNeeded
    } else {
        choices
    }
}

#[must_use]
pub const fn launch_prompt_should_probe_agents(default_agent_configured: bool) -> bool {
    !default_agent_configured
}

#[derive(Debug)]
pub struct LaunchAgentPromptPlan {
    pub outcome: PromptOutcome,
    pub store_pending_launch: bool,
    pub error: Option<anyhow::Error>,
}
