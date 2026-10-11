// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! Background events and instance actions.

#[derive(Debug)]
pub enum BackgroundEvent<M, RoleLoad, DriftCheck, DriftDetection, IsolationCleanup, ConfigSave> {
    Message(M),
    RoleLoadFinished {
        load: RoleLoad,
        result: anyhow::Result<()>,
    },
    DriftCheckFinished {
        check: DriftCheck,
        detection: anyhow::Result<DriftDetection>,
    },
    IsolationCleanupFinished {
        cleanup: IsolationCleanup,
        result: anyhow::Result<()>,
    },
    ConfigSaveFinished(ConfigSave),
}

#[derive(Debug)]
pub enum ConsoleInputOutcome<RoleSelector, Agent, InstanceAction, Selection> {
    Continue,
    ExitJackin,
    LaunchNamed(String),
    PrewarmNamed(String),
    LaunchCurrentDir,
    LaunchWithAgent(RoleSelector),
    LaunchWithRuntimeAgent(Agent),
    InstanceAction {
        container: String,
        action: InstanceAction,
    },
    NewSessionWithAccount {
        container: String,
        agent: Agent,
        instance_id: String,
    },
    LaunchWithAccount {
        selector: RoleSelector,
        agent: Agent,
        selection: Selection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleInstanceAction<Agent> {
    Reconnect,
    /// Reconnect and ask the in-container daemon to focus this pane
    /// (`session_id`) before forwarding output.
    ReconnectFocus(u64),
    NewSession,
    NewSessionWithAgent(Agent),
    Shell,
    Inspect,
    Stop,
    Purge,
}

impl<Agent> ConsoleInstanceAction<Agent> {
    /// Actions that do not replace the TUI with another foreground process.
    pub fn runs_in_place(self) -> bool {
        matches!(self, Self::Stop | Self::Purge)
    }

    pub fn workspace_action_fact(
        self,
    ) -> crate::tui::screens::workspaces::update::WorkspaceInstanceAction {
        use crate::tui::screens::workspaces::update::WorkspaceInstanceAction;

        match self {
            Self::Reconnect | Self::ReconnectFocus(_) => WorkspaceInstanceAction::Reconnect,
            Self::NewSession | Self::NewSessionWithAgent(_) => WorkspaceInstanceAction::NewSession,
            Self::Shell => WorkspaceInstanceAction::Shell,
            Self::Inspect => WorkspaceInstanceAction::Inspect,
            Self::Stop => WorkspaceInstanceAction::Stop,
            Self::Purge => WorkspaceInstanceAction::Purge,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleOutcome<RoleSelector, Workspace, Agent, Selection> {
    Launch(RoleSelector, Workspace, Option<Agent>),
    PrewarmNamed(String),
    InstanceAction {
        container: String,
        action: ConsoleInstanceAction<Agent>,
    },
    /// Operator selected an agent and a provider in the console picker.
    NewSessionWithAccount {
        container: String,
        agent: Agent,
        instance_id: String,
    },
    /// Initial launch with a provider selected before the container exists.
    LaunchWithAccount {
        selector: RoleSelector,
        workspace: Workspace,
        agent: Agent,
        selection: Selection,
    },
}

pub trait InstanceActionHandler<Agent> {
    async fn run_in_place(
        &mut self,
        container: &str,
        action: ConsoleInstanceAction<Agent>,
    ) -> anyhow::Result<()>;
}
