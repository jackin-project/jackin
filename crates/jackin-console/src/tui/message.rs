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
