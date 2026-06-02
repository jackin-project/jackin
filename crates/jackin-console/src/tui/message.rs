//! Top-level console TUI message helpers.
//!
//! Product-specific manager messages still live in the root crate while the
//! workspace console owns root-only config/runtime types. Generic message
//! carriers live here so the top-level TUI vocabulary has a home in the
//! surface crate.

#[derive(Debug)]
pub enum BackgroundEvent<M, RoleLoad, DriftCheck, DriftDetection, IsolationCleanup> {
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
}

#[derive(Debug)]
pub enum ConsoleInputOutcome<RoleSelector, Agent, InstanceAction, Provider> {
    Continue,
    ExitJackin,
    LaunchNamed(String),
    LaunchCurrentDir,
    LaunchWithAgent(RoleSelector),
    LaunchWithRuntimeAgent(Agent),
    InstanceAction {
        container: String,
        action: InstanceAction,
    },
    NewSessionWithProvider {
        container: String,
        agent: Agent,
        provider: Provider,
    },
    LaunchWithProvider {
        selector: RoleSelector,
        agent: Agent,
        provider: Provider,
    },
}

pub enum ConsolePreludeModalOutcome {
    Continue,
    OpenUrl(String),
    ReopenFileBrowserAtLastCwd,
    ApplyFileBrowserOutcome {
        outcome: crate::tui::components::file_browser::FileBrowserOutcome<std::path::PathBuf>,
        browser_cwd: Option<std::path::PathBuf>,
    },
    ResolveFileBrowserGitUrl(std::path::PathBuf),
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
    ApplyFileBrowserOutcome(
        crate::tui::components::file_browser::FileBrowserOutcome<std::path::PathBuf>,
    ),
    ResolveFileBrowserGitUrl(std::path::PathBuf),
    OpenUrl(String),
    ValidateOpRef(OpRef),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConsoleSettingsModalOutcome {
    Continue,
    SaveSettings,
    OpenGlobalMountFileBrowser,
    OpenUrl(String),
    ApplyFileBrowserOutcome(
        crate::tui::components::file_browser::FileBrowserOutcome<std::path::PathBuf>,
    ),
    ResolveFileBrowserGitUrl(std::path::PathBuf),
}

#[derive(Debug)]
pub enum ConsoleSettingsAuthOutcome<OpRef> {
    Continue,
    ValidateOpRef(OpRef),
}
