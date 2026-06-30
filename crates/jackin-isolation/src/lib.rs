//! jackin-isolation: mount isolation subsystem.
//!
//! # Architecture Invariant
//!
//! Allowed production dependencies (inward only):
//! - `jackin-core` (domain types, `CommandRunner`, constants, `worktree_dirty`)
//! - `jackin-config` (workspace config types: `ResolvedWorkspace`, `DirtyExitPolicy`)
//! - `jackin-diagnostics` (debug telemetry macros)
//! - `jackin-protocol` (`ExitAction` for finalize)
//! - `jackin-docker` (`DockerApi` for finalize)
//!
//! Must NOT depend on: `jackin-runtime`, `jackin-launch`, `jackin-tui`,
//! `jackin-docker` (docker calls are in materialize via `CommandRunner` trait).
//!
//! Three isolation strategies: `Shared` (read-write bind), `Worktree` (git
//! worktree clone, finalized post-attach), `Clone` (full directory copy,
//! finalized post-attach). Sub-modules: `materialize` (bind-spec production),
//! `cleanup` (forced removal), `state` (`IsolationRecord` persistence),
//! `branch` (worktree branch naming).
//!
//! R3 completed: `finalize` and `git_inspect` moved here from jackin-runtime
//! (re-export shims preserve call sites). Stale note removed.

pub mod branch;
pub mod cleanup;
pub mod finalize;
pub mod git_inspect;
pub mod materialize;
pub mod state;

pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;
