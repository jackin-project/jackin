//! jackin-core: universal vocabulary types shared across all jackin❯ crates.
//!
//! **Architecture Invariant:** T0.
//! Entry point: [`Agent`] — primary domain noun re-exported to every crate.

// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![deny(
    clippy::string_slice,
    clippy::indexing_slicing,
    clippy::get_unwrap,
    clippy::unwrap_in_result,
    clippy::panic_in_result_fn,
    clippy::unchecked_time_subtraction
)]
#![deny(missing_docs)]

mod account_key;
mod agent;
mod ansi_tokens;
mod auth;
mod build_log_sink;
mod clock;
mod constants;
mod container_id;
/// Container-side path constants. Kept as a `pub mod` so call sites can
/// `use jackin_core::container_paths` as a namespace (many sites; plan 019).
pub mod container_paths;
/// Debug-log sink + `debug_log!` macro. Kept as a `pub mod` because
/// `#[macro_export] debug_log!` shares the `jackin_core::debug_log` path with
/// the module name (plan 019 justified remainder).
pub mod debug_log;
mod docker;
mod docker_security;
mod env_model;
mod env_value;
mod host_colors;
mod instance;
mod isolation;
mod isolation_record;
mod launch_progress;
mod manifest;
mod modal_outcome;
mod op_cache;
mod op_probe_error;
mod op_reference;
mod op_types;
mod operator_notice;
mod path_text;
mod paths;
mod prompt_result;
mod runner;
mod selector;
mod session_id;
mod standalone_dialog;
mod status;
mod url_text;
mod workspace_label;
mod workspace_name;
mod worktree_dirty;

pub use account_key::*;
pub use agent::{Agent, AgentRuntime, AgentStatePaths, ParseAgentError, agent_runtime_registry};
pub use ansi_tokens::*;
pub use auth::*;
pub use build_log_sink::*;
pub use clock::*;
pub use constants::*;
pub use container_id::*;
pub use container_paths::*;
pub use debug_log::{DebugLogSink, emit_debug_line, is_debug_mode, set_global_sink};
// Note: `set_global_sink` for operator notices is not re-exported at root (name
// collision); use `operator_notice::set_global_sink` via the notice helpers or
// the diagnostics bridge.
pub use docker::*;
pub use docker_security::*;
pub use env_model::*;
pub use env_value::*;
pub use host_colors::*;
pub use instance::*;
pub use isolation::*;
pub use isolation_record::*;
pub use launch_progress::*;
pub use manifest::*;
pub use modal_outcome::*;
pub use op_cache::*;
pub use op_probe_error::*;
pub use op_reference::*;
pub use op_types::*;
pub use operator_notice::{
    OperatorNoticeSink, emit_compact_line, set_global_sink as set_operator_notice_sink,
};
pub use path_text::*;
pub use paths::*;
pub use prompt_result::*;
pub use runner::*;
pub use selector::*;
pub use session_id::*;
pub use standalone_dialog::*;
pub use status::*;
pub use url_text::*;
pub use workspace_label::*;
pub use workspace_name::*;
pub use worktree_dirty::*;
