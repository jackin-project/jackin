//! jackin-runtime-coordination: lock files and state directories for runtime coordination.
//!
//! **Architecture Invariant:** T1.
//! Entry point: [`coordination::open_lock`] — namespaced lock acquisition.
//!
//! Owns the on-disk coordination surface every runtime instance shares:
//! state roots, universe directories, and advisory lock files (unix uses
//! `openat` + ownership checks, other platforms fall back to plain files).
//! Split out of `jackin-runtime` (S7 split 44); the old
//! `jackin_runtime::runtime::coordination::*` paths keep working through a
//! re-export shim.

pub mod coordination;
