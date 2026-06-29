//! jackin-core: universal vocabulary types shared across all jackin❯ crates.
//!
//! This is a leaf crate — it has no jackin❯ dependencies, no tokio, no
//! subprocess, no filesystem access. Every higher crate depends on this one,
//! never the reverse.
//!
//! **Architecture Invariant:** L0 domain crate. Allowed dependencies: none
//! inside the workspace. Owns the universal types plus the port traits
//! (`BuildLogSink`, `DebugLogSink`, `OperatorNoticeSink`) and shared widget
//! stubs (`TailScroll`, `DialogBodyScroll`, etc.) that let higher layers
//! talk upward without depending on a presentation crate.
//!
//! Public surface: `Agent`, `MountIsolation`, `AuthForwardMode`, and shared
//! string constants.

pub mod account_key;
pub mod agent;
pub mod auth;
pub mod build_log_sink;
pub mod constants;
pub mod debug_log;
pub mod docker;
pub mod docker_security;
pub mod env_model;
pub mod env_value;
pub mod host_colors;
pub mod instance;
pub mod isolation;
pub mod isolation_record;
pub mod launch_progress;
pub mod manifest;
pub mod op_cache;
pub mod op_reference;
pub mod op_types;
pub mod operator_notice;
pub mod path_text;
pub mod paths;
pub mod prompt_result;
pub mod runner;
pub mod selector;
pub mod tui_widgets;
pub mod url_text;
pub mod worktree_dirty;

pub use agent::{
    Agent, ParseAgentError,
    adapters::registry as agent_runtime_registry,
    runtime::{AgentRuntime, AgentStatePaths},
};
pub use auth::AuthForwardMode;
pub use build_log_sink::BuildLogSink;
pub use debug_log::{DebugLogSink, emit_debug_line, is_debug_mode, set_global_sink};
pub use docker::{
    ContainerRow, ContainerSpec, ContainerState, DockerApi, NetworkRow, RemoveImageOutcome,
};
pub use docker_security::{
    DindGrant, DockerGrants, DockerSecurityProfile, NetworkGrant, ParseProfileError,
};
pub use env_value::{EnvValue, Extended, FieldTarget, OpRef};
pub use host_colors::{PHOSPHOR_DARK, PHOSPHOR_DIM, PHOSPHOR_GREEN, Rgb, owo_rgb};
pub use isolation::{MountIsolation, ParseMountIsolationError};
pub use isolation_record::{CleanupStatus, DriftDetection, IsolationRecord};
pub use launch_progress::{
    FailureCopyTarget, FileDiff, LaunchCancelled, LaunchCandidate, LaunchDiagnostics,
    LaunchDialogResult, LaunchFailure, LaunchHostTerminal, LaunchIdentity, LaunchStage,
    LaunchTargetKind, PromptContextLine, StageLabelTransition, StageStatus, StageView,
    WorktreeInspect,
};
pub use operator_notice::{OperatorNoticeSink, emit_compact_line};
pub use path_text::shorten_home;
pub use paths::JackinPaths;
pub use prompt_result::PromptResult;
pub use runner::{CommandRunner, RunOptions};
pub use selector::{RoleSelector, Selector, SelectorError, runtime_slug};
pub use tui_widgets::{
    BOTTOM_CHROME_ROWS, BottomChromeAreas, DialogBodyScroll, StatusFooterHover, TailScroll,
    bottom_chrome_areas, is_scrollable, max_line_width, max_offset,
};
pub use url_text::{has_url_scheme, is_host_open_url, redact_url_for_log};
