//! jackin-runtime-apple-container-supervisor-env: supervisor env builder.
//!
//! **Architecture Invariant:** T2.
//! Entry point: [`supervisor_env::apple_supervisor_env`] —
//! build the supervisor env pairs for the launch spec.
//!
//! Split out of `jackin-runtime` (S7 split 114): the supervisor
//! env builder in the apple-container `launch` path.
//! The hub keeps a private import; the step had no external
//! callers.

pub mod supervisor_env;
