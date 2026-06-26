//! jackin-core: universal vocabulary types shared across all jackin' crates.
//!
//! This is a leaf crate — it has no jackin' dependencies, no tokio, no
//! subprocess, no filesystem access. Every higher crate depends on this one,
//! never the reverse.
//!
//! Public surface: `Agent`, `MountIsolation`, `AuthForwardMode`, and shared
//! string constants.

pub mod account_key;
pub mod agent;
pub mod ansi_text;
pub mod auth;
pub mod constants;
pub mod docker;
pub mod docker_security;
pub mod env_model;
pub mod env_value;
pub mod instance;
pub mod isolation;
pub mod isolation_record;
pub mod manifest;
pub mod op_cache;
pub mod op_reference;
pub mod op_types;
pub mod path_text;
pub mod paths;
pub mod prune_output;
pub mod runner;
pub mod selector;
pub mod url_text;
pub mod worktree_dirty;

pub use agent::{
    Agent, ParseAgentError,
    adapters::registry as agent_runtime_registry,
    runtime::{AgentRuntime, AgentStatePaths},
};
pub use auth::AuthForwardMode;
pub use docker::{
    ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
};
pub use docker_security::{
    DindGrant, DockerGrants, DockerSecurityProfile, NetworkGrant, ParseProfileError,
};
pub use env_value::{EnvValue, FieldTarget, OpRef};
pub use isolation::{MountIsolation, ParseMountIsolationError};
pub use isolation_record::{CleanupStatus, DriftDetection, IsolationRecord};
pub use path_text::shorten_home;
pub use paths::JackinPaths;
pub use runner::{CommandRunner, RunOptions};
pub use selector::{RoleSelector, Selector, SelectorError};
