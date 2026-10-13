//! jackin-runtime-launch-mounts: launch bind-mount assembly per backend.
//!
//! **Architecture Invariant:** T5.
//! Entry point: [`mounts::build_workspace_mounts`] — workspace bind-mount assembly.
//!
//! Resolves the container backend and assembles every bind mount for
//! a launch: workspace mounts, per-agent data/config mounts, provider
//! authority guards, and Apple-container mount specs. Split out of
//! `jackin-runtime` (S7 split 61); the old
//! `jackin_runtime::runtime::launch::mounts::*` paths keep working
//! through a re-export shim.

pub mod mounts;
