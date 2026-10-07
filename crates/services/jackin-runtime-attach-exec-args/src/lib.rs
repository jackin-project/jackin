//! jackin-runtime-attach-exec-args: exec arg builders.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`exec_args::git_policy_env_pairs`] — env pairs.
//!
//! Role exec arg builders: terminal titles, run-as user,
//! alt-screen flags, and git policy env. Split out of
//! `jackin-runtime` (S7 split 79); the old
//! `jackin_runtime::runtime::attach::exec_args::*` paths keep
//! working through a module re-export in `attach.rs`.

pub mod exec_args;
