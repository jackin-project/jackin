//! jackin-runtime-host-daemon: host daemon backend over a Unix socket.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`host_daemon::serve`] — daemon request loop.
//!
//! Serves daemon RPCs (snapshots, agent state, telemetry context) to
//! attached sessions over a Unix-domain socket. Unix-only. Split out of
//! `jackin-runtime` (S7 split 43); the old
//! `jackin_runtime::host_daemon::*` paths keep working through a
//! re-export shim.

#[cfg(unix)]
pub mod host_daemon;
