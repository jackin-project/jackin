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
    /// Stay in the manager.
    Continue,
    /// Exit jackin entirely from the manager list.
    ExitJackin,
    /// Launch the named workspace; resolved by name in the run loop.
    LaunchNamed(String),
    /// Launch against the synthetic current-directory choice.
    LaunchCurrentDir,
    /// Operator committed a role choice in the launch picker.
    LaunchWithAgent(RoleSelector),
    /// Operator committed a runtime agent after choosing a role.
    LaunchWithRuntimeAgent(Agent),
    /// Run an instance recovery action selected from the console.
    InstanceAction {
        container: String,
        action: InstanceAction,
    },
    /// Open an external URL. The root run loop executes this because browser
    /// launching is a host-side side effect, not input/update work.
    OpenUrl(String),
    /// Operator selected an agent and provider for a new session.
    NewSessionWithProvider {
        container: String,
        agent: Agent,
        provider: Provider,
    },
    /// Operator selected a provider for the initial workspace launch.
    LaunchWithProvider {
        selector: RoleSelector,
        agent: Agent,
        provider: Provider,
    },
}
