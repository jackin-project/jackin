//! jackin-runtime-launch-git-pull: workspace repo git pull.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`git_pull::pull_git_sources_with_git`] — pull driver.
//!
//! Git pull helpers for workspace repos: source discovery from mounts,
//! threaded pull execution, and result printing/recording. Split out of
//! `jackin-runtime` (S7 split 69); the old
//! `jackin_runtime::runtime::launch::git_pull::*` paths keep working
//! through a module re-export in `launch.rs`.

pub mod git_pull;
