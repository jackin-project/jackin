//! jackin-runtime-exec-host: host-side credential resolver for jackin-exec.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`exec_host::start_for_container`] — socket listener for a container.
//!
//! Listens on a Unix socket bind-mounted into role containers so the
//! capsule daemon can resolve on-demand credential env vars before running
//! commands. Split out of `jackin-runtime` (S7 split 42); the old
//! `jackin_runtime::exec_host::*` paths keep working through a re-export
//! shim.

pub mod exec_host;
