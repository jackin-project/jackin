//! jackin-isolation: mount isolation subsystem.
//!
//! # Architecture Invariant
//!
//! Allowed production dependencies (inward only):
//! - `jackin-core` (domain types, `CommandRunner`, constants, `worktree_dirty`)
//! - `jackin-config` (workspace config types: `ResolvedWorkspace`, `DirtyExitPolicy`)
//! - `jackin-diagnostics` (debug telemetry macros)
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
//! Note: `finalize.rs` and `git_inspect.rs` remain under
//! `jackin_runtime::isolation`. Their exit-dialog and error-popup calls now
//! route through the L0 port traits (`jackin_core::exit_dialog_with_inspect` /
//! `jackin_core::error_popup`) instead of directly into `jackin_launch_tui`,
//! so the L1→L3 inversion that originally parked them is closed. A full
//! move into this crate is left for a follow-up slice because the in-place
//! tests rely on `jackin_runtime::test_support::FakeRunner` and dropping a
//! duplicate here is mechanical cleanup beyond this slice's scope.

pub mod branch;
pub mod cleanup;
pub mod materialize;
pub mod state;

pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;
