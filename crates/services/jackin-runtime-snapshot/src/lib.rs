//! jackin-runtime-snapshot: capsule snapshot and usage fetch over control socket.
//!
//! **Architecture Invariant:** T4.
//! Entry point: [`snapshot::fetch_snapshot`] — socket read with exec fallback.
//!
//! Reads instance snapshots and usage accounts from the capsule control
//! socket, falling back to `docker exec` when the socket is absent. Split
//! out of `jackin-runtime` (S7 split 46); the old
//! `jackin_runtime::runtime::snapshot::*` paths keep working through a
//! re-export shim.

pub mod snapshot;
