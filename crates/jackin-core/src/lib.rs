//! jackin-core: universal vocabulary types shared across all jackin❯ crates.
//!
//! This is a leaf crate — it has no jackin❯ dependencies, no tokio, no
//! subprocess, no filesystem access. Every higher crate depends on this one,
//! never the reverse.
//!
//! Public surface: `Agent`, `MountIsolation`, `AuthForwardMode`, and shared
//! string constants.

pub mod account_key;
pub mod agent;
pub mod auth;
pub mod build_log_sink;
pub mod constants;
pub mod docker;
pub mod docker_security;
pub mod env_model;
pub mod env_value;
pub mod instance;
pub mod isolation;
pub mod isolation_record;
pub mod launch_progress;
pub mod manifest;
pub mod op_cache;
pub mod op_reference;
pub mod op_types;
pub mod path_text;
pub mod paths;
pub mod prompt_result;
pub mod runner;
pub mod selector;
pub mod worktree_dirty;

pub use agent::{
    Agent, ParseAgentError,
    adapters::registry as agent_runtime_registry,
    runtime::{AgentRuntime, AgentStatePaths},
};
pub use auth::AuthForwardMode;
pub use build_log_sink::BuildLogSink;
pub use docker::{
    ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
};
pub use docker_security::{
    DindGrant, DockerGrants, DockerSecurityProfile, NetworkGrant, ParseProfileError,
};
pub use env_value::{EnvValue, Extended, FieldTarget, OpRef};
pub use isolation::{MountIsolation, ParseMountIsolationError};
pub use isolation_record::{CleanupStatus, DriftDetection, IsolationRecord};
pub use launch_progress::{
    FailureCopyTarget, FileDiff, LaunchCancelled, LaunchCandidate, LaunchDiagnostics,
    LaunchDialogResult, LaunchFailure, LaunchHostTerminal, LaunchIdentity, LaunchStage,
    LaunchTargetKind, PromptContextLine, StageLabelTransition, StageStatus, StageView,
    WorktreeInspect,
};
pub use path_text::shorten_home;
pub use paths::JackinPaths;
pub use prompt_result::PromptResult;
pub use runner::{CommandRunner, RunOptions};
pub use selector::{RoleSelector, Selector, SelectorError, runtime_slug};
