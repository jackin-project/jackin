//! Mount isolation: `MountIsolation` enum and the sub-modules that implement
//! the three isolation strategies.
//!
//! * `Shared` — read-write bind mount of the host path; no git operations.
//! * `Worktree` — git-worktree clone of the host repo, finalized post-attach.
//! * `Clone` — full directory copy, finalized post-attach.
//!
//! Sub-modules: `materialize` (produces bind specs from `WorkspaceConfig`),
//! `finalize` (post-attach preserve-vs-clean policy), `cleanup` (forced
//! removal), `state` (`IsolationRecord` persistence), `branch` (worktree
//! branch naming).
//!
//! The 4 production modules (branch / cleanup / materialize / state) now
//! live in the `jackin-isolation` crate (C2 carve); they are re-exported
//! here so existing `crate::isolation::branch::*` etc. call sites keep
//! compiling without edits. `finalize` and `git_inspect` remain in
//! `jackin-runtime` for now — they reach into `jackin_runtime::runtime
//! ::attach` / `jackin_launch_tui::launch_progress` symbols and lifting
//! those out requires a preparatory slice.

pub mod finalize;
pub mod git_inspect;

pub mod branch {
    pub use jackin_isolation::branch::*;
}
pub mod cleanup {
    pub use jackin_isolation::cleanup::*;
}
pub mod materialize {
    pub use jackin_isolation::materialize::*;
}
pub mod state {
    pub use jackin_isolation::state::*;
}

pub use jackin_core::MountIsolation;
pub use jackin_core::ParseMountIsolationError;

#[cfg(test)]
mod tests;
