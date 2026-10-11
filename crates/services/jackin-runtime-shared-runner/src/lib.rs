//! jackin-runtime-shared-runner: cloneable serialized runner handle.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`shared_runner::SharedCommandRunner`] — shared handle.
//!
//! Cloneable serialized wrapper for the mutable `CommandRunner` seam.
//! Split out of `jackin-runtime` (S7 split 81); the hub held zero
//! in-repo call sites, so no re-export shim remains.

pub mod shared_runner;
