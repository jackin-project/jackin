//! jackin-runtime-cleanup-socket-dir: host-side socket-directory removal.
//!
//! **Architecture Invariant:** T6.
//! Entry point: [`socket_dir::remove_socket_dir`] —
//! remove the per-container `sockets/<name>/` directory.
//!
//! Split out of `jackin-runtime` (S7 split 105): the shared
//! best-effort socket-dir teardown (coordination gate plus the
//! contained safe-remove helper), decoupled from the purge and
//! eject flows that share it. The old
//! `jackin_runtime::runtime::cleanup::remove_socket_dir` path
//! keeps working through the hub re-export.

pub mod socket_dir;
