//! jackin-runtime-launch-run-envs: run invocation env helper.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`run_envs::run_runtime_envs`] —
//! run env strings for the launch run args.
//!
//! Split out of `jackin-runtime` (S7 split 116): the run
//! invocation env helper in the launch runtime `run` path.
//! The hub keeps a `pub(crate)` re-export; the step
//! stays exercised through the same launch path.

pub mod run_envs;
