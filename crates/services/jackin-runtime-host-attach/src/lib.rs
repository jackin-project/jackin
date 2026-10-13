//! jackin-runtime-host-attach: host attach client.
//!
//! **Architecture Invariant:** T7.
//! Entry point: [`host_attach::run_host_attach_session`].
//!
//! Host-owned attach client for running Capsule daemons:
//! the operator terminal twin of the in-container
//! interactive client. Split out of `jackin-runtime` (S7
//! split 76); the old
//! `jackin_runtime::runtime::host_attach::*` paths keep
//! working through a glob re-export in `host_attach.rs`.

pub mod host_attach;
